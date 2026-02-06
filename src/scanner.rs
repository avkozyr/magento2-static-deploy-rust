//! Theme and module scanning for Magento 2 installations.
//!
//! Discovers themes in `app/design/` and modules in `vendor/` using
//! parallel iteration with Rayon for improved performance.

use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::error::DeployError;
use crate::theme::{detect_theme_type, parse_theme_xml, Area, Theme};

/// Read module name from etc/module.xml
fn get_module_name(package_path: &Path) -> Option<String> {
    // Try etc/module.xml first
    let mut module_xml_path = package_path.join("etc").join("module.xml");
    if !module_xml_path.exists() {
        // Try src/etc/module.xml
        module_xml_path = package_path.join("src").join("etc").join("module.xml");
        if !module_xml_path.exists() {
            return None;
        }
    }

    let content = fs::read_to_string(&module_xml_path).ok()?;
    parse_module_xml(&content)
}

/// Parse module name from module.xml content
fn parse_module_xml(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.name().as_ref() == b"module" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"name" {
                        return attr.unescape_value().ok().map(|s| s.to_string());
                    }
                }
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

/// Origin of static files for a theme
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum FileSource {
    /// Theme's own web directory: app/design/{area}/{Vendor}/{theme}/web/
    ThemeWeb { theme: String, path: PathBuf },
    /// Library files: lib/web/
    Library { path: PathBuf },
    /// Vendor module assets: vendor/{vendor}/{module}/view/{area}/web/
    VendorModule { module: String, path: PathBuf },
    /// Theme module override: app/design/{area}/{Vendor}/{theme}/{Module}/web/
    ThemeModuleOverride {
        theme: String,
        module: String,
        path: PathBuf,
    },
}

/// Discover all themes in app/design/{area}/ using parallel iteration
#[must_use = "this returns the discovered themes which should be processed"]
pub fn discover_themes(magento_root: &Path, area: Area) -> Result<Vec<Theme>, DeployError> {
    let design_path = magento_root.join("app").join("design").join(area.as_str());

    if !design_path.exists() {
        return Ok(Vec::new());
    }

    // Collect vendor directories first
    let vendor_dirs: Vec<_> = fs::read_dir(&design_path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    // Process vendors in parallel
    let themes: Vec<Theme> = vendor_dirs
        .par_iter()
        .flat_map(|vendor_entry| {
            let vendor_path = vendor_entry.path();
            let vendor = vendor_entry.file_name().to_string_lossy().to_string();

            // Collect theme dirs for this vendor
            let theme_dirs: Vec<_> = fs::read_dir(&vendor_path)
                .ok()
                .into_iter()
                .flat_map(|rd| rd.filter_map(|e| e.ok()))
                .filter(|e| e.path().is_dir())
                .collect();

            // Process themes within vendor in parallel
            theme_dirs
                .par_iter()
                .filter_map(|theme_entry| {
                    let theme_path = theme_entry.path();
                    let theme_xml_path = theme_path.join("theme.xml");

                    if !theme_xml_path.exists() {
                        return None;
                    }

                    let name = theme_entry.file_name().to_string_lossy().to_string();

                    // Parse theme.xml for parent
                    let xml_content = fs::read_to_string(&theme_xml_path).ok()?;
                    let parent = parse_theme_xml(&xml_content);

                    // Build parent chain names for detection
                    let parent_names: Vec<String> =
                        parent.iter().map(|p| p.as_str().to_string()).collect();
                    let theme_type = detect_theme_type(&xml_content, &parent_names);

                    Some(Theme {
                        vendor: vendor.clone(),
                        name,
                        area,
                        path: theme_path,
                        parent,
                        theme_type,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();

    Ok(themes)
}

/// Scan theme's web directory for static files
pub fn scan_theme_web_sources(theme: &Theme) -> Vec<FileSource> {
    let web_path = theme.path.join("web");
    if web_path.exists() {
        vec![FileSource::ThemeWeb {
            theme: theme.full_name(),
            path: web_path,
        }]
    } else {
        Vec::new()
    }
}

/// Scan lib/web/ for shared library assets
pub fn scan_library_sources(magento_root: &Path) -> Vec<FileSource> {
    let lib_path = magento_root.join("lib").join("web");
    if lib_path.exists() {
        vec![FileSource::Library { path: lib_path }]
    } else {
        Vec::new()
    }
}

/// Scan vendor modules for static assets using parallel iteration
pub fn scan_vendor_module_sources(magento_root: &Path, area: Area) -> Vec<FileSource> {
    let vendor_path = magento_root.join("vendor");
    if !vendor_path.exists() {
        return Vec::new();
    }

    // Collect vendor directories first
    let vendor_dirs: Vec<_> = WalkDir::new(&vendor_path)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .collect();

    // Process vendors in parallel
    vendor_dirs
        .par_iter()
        .flat_map(|vendor_entry| {
            // Collect module directories for this vendor
            let module_dirs: Vec<_> = WalkDir::new(vendor_entry.path())
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_dir())
                .collect();

            // Process modules in parallel
            module_dirs
                .par_iter()
                .flat_map(|module_entry| {
                    let package_path = module_entry.path();

                    // Get proper module name from etc/module.xml
                    let Some(module_name) = get_module_name(package_path) else {
                        return Vec::new();
                    };

                    // Pre-allocate for typical module sources (1-4 paths)
                    let mut sources = Vec::with_capacity(4);

                    // Check standard path: view/{area}/web
                    let web_path = package_path.join("view").join(area.as_str()).join("web");
                    if web_path.exists() {
                        sources.push(FileSource::VendorModule {
                            module: module_name.clone(),
                            path: web_path,
                        });
                    }

                    // Check Hyva-style path: src/view/{area}/web
                    let src_web_path = package_path
                        .join("src")
                        .join("view")
                        .join(area.as_str())
                        .join("web");
                    if src_web_path.exists() {
                        sources.push(FileSource::VendorModule {
                            module: module_name.clone(),
                            path: src_web_path,
                        });
                    }

                    // Check base area: view/base/web (shared across all areas)
                    let base_web_path = package_path.join("view").join("base").join("web");
                    if base_web_path.exists() {
                        sources.push(FileSource::VendorModule {
                            module: module_name.clone(),
                            path: base_web_path,
                        });
                    }

                    // Check Hyva-style base: src/view/base/web
                    let src_base_web_path = package_path
                        .join("src")
                        .join("view")
                        .join("base")
                        .join("web");
                    if src_base_web_path.exists() {
                        sources.push(FileSource::VendorModule {
                            module: module_name.clone(),
                            path: src_base_web_path,
                        });
                    }

                    sources
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Scan theme module overrides in app/design/{area}/{Vendor}/{theme}/{Module_Name}/web/
pub fn scan_theme_module_overrides(theme: &Theme) -> Vec<FileSource> {
    // Pre-allocate for typical theme overrides (5-10 modules)
    let mut sources = Vec::with_capacity(8);

    for entry in WalkDir::new(&theme.path)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy();

        // Module directories contain underscore (e.g., Magento_Catalog)
        if !dir_name.contains('_') {
            continue;
        }

        let web_path = entry.path().join("web");
        if web_path.exists() {
            sources.push(FileSource::ThemeModuleOverride {
                theme: theme.full_name(),
                module: dir_name.to_string(),
                path: web_path,
            });
        }
    }

    sources
}

/// Collect all file sources for a theme with proper priority order
pub fn collect_file_sources(
    theme: &Theme,
    parent_chain: &[&Theme],
    magento_root: &Path,
) -> Vec<FileSource> {
    // Pre-allocate for typical source count (50-200 sources)
    let mut sources = Vec::with_capacity(100);

    // Priority order (highest first):
    // 1. Theme module overrides (current theme)
    sources.extend(scan_theme_module_overrides(theme));

    // 2. Theme web (current theme)
    sources.extend(scan_theme_web_sources(theme));

    // 3. Parent themes (in order)
    for parent in parent_chain {
        sources.extend(scan_theme_module_overrides(parent));
        sources.extend(scan_theme_web_sources(parent));
    }

    // 4. Vendor module assets
    sources.extend(scan_vendor_module_sources(magento_root, theme.area));

    // 5. Library assets
    sources.extend(scan_library_sources(magento_root));

    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ThemeCode, ThemeType};
    use std::fs;
    use tempfile::TempDir;

    // ==================== parse_module_xml tests ====================

    #[test]
    fn test_parse_module_xml_valid() {
        let xml = r#"<?xml version="1.0"?>
<config>
    <module name="Vendor_ModuleName" setup_version="1.0.0"/>
</config>"#;

        let result = parse_module_xml(xml);
        assert_eq!(result, Some("Vendor_ModuleName".to_string()));
    }

    #[test]
    fn test_parse_module_xml_start_tag() {
        let xml = r#"<?xml version="1.0"?>
<config>
    <module name="Test_Module">
        <sequence/>
    </module>
</config>"#;

        let result = parse_module_xml(xml);
        assert_eq!(result, Some("Test_Module".to_string()));
    }

    #[test]
    fn test_parse_module_xml_no_module() {
        let xml = r#"<?xml version="1.0"?>
<config></config>"#;

        let result = parse_module_xml(xml);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_module_xml_no_name_attribute() {
        let xml = r#"<?xml version="1.0"?>
<config>
    <module version="1.0.0"/>
</config>"#;

        let result = parse_module_xml(xml);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_module_xml_malformed() {
        let xml = "not valid xml <<<<";
        let result = parse_module_xml(xml);
        assert_eq!(result, None);
    }

    // ==================== get_module_name tests ====================

    #[test]
    fn test_get_module_name_standard_path() {
        let temp = TempDir::new().unwrap();
        let etc_path = temp.path().join("etc");
        fs::create_dir_all(&etc_path).unwrap();

        fs::write(
            etc_path.join("module.xml"),
            r#"<?xml version="1.0"?>
<config><module name="Test_StandardPath"/></config>"#,
        )
        .unwrap();

        let result = get_module_name(temp.path());
        assert_eq!(result, Some("Test_StandardPath".to_string()));
    }

    #[test]
    fn test_get_module_name_src_path() {
        let temp = TempDir::new().unwrap();
        let src_etc = temp.path().join("src").join("etc");
        fs::create_dir_all(&src_etc).unwrap();

        fs::write(
            src_etc.join("module.xml"),
            r#"<?xml version="1.0"?>
<config><module name="Test_SrcPath"/></config>"#,
        )
        .unwrap();

        let result = get_module_name(temp.path());
        assert_eq!(result, Some("Test_SrcPath".to_string()));
    }

    #[test]
    fn test_get_module_name_no_module_xml() {
        let temp = TempDir::new().unwrap();
        let result = get_module_name(temp.path());
        assert_eq!(result, None);
    }

    // ==================== discover_themes tests ====================

    #[test]
    fn test_discover_themes_nonexistent_path() {
        let temp = TempDir::new().unwrap();
        let result = discover_themes(temp.path(), Area::Frontend).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_themes_single_theme() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp
            .path()
            .join("app")
            .join("design")
            .join("frontend")
            .join("TestVendor")
            .join("testtheme");
        fs::create_dir_all(&theme_path).unwrap();

        fs::write(
            theme_path.join("theme.xml"),
            r#"<?xml version="1.0"?>
<theme><title>Test Theme</title></theme>"#,
        )
        .unwrap();

        let result = discover_themes(temp.path(), Area::Frontend).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vendor, "TestVendor");
        assert_eq!(result[0].name, "testtheme");
        assert_eq!(result[0].area, Area::Frontend);
    }

    #[test]
    fn test_discover_themes_multiple_themes() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("app").join("design").join("frontend");

        // Create two themes in same vendor
        let theme1 = base.join("Vendor").join("theme1");
        let theme2 = base.join("Vendor").join("theme2");
        fs::create_dir_all(&theme1).unwrap();
        fs::create_dir_all(&theme2).unwrap();

        fs::write(
            theme1.join("theme.xml"),
            r#"<theme><title>T1</title></theme>"#,
        )
        .unwrap();
        fs::write(
            theme2.join("theme.xml"),
            r#"<theme><title>T2</title></theme>"#,
        )
        .unwrap();

        let result = discover_themes(temp.path(), Area::Frontend).unwrap();

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_discover_themes_with_parent() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp
            .path()
            .join("app")
            .join("design")
            .join("frontend")
            .join("Child")
            .join("theme");
        fs::create_dir_all(&theme_path).unwrap();

        fs::write(
            theme_path.join("theme.xml"),
            r#"<?xml version="1.0"?>
<theme>
    <title>Child Theme</title>
    <parent>Hyva/default</parent>
</theme>"#,
        )
        .unwrap();

        let result = discover_themes(temp.path(), Area::Frontend).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].parent, Some(ThemeCode::from("Hyva/default")));
    }

    #[test]
    fn test_discover_themes_skips_dirs_without_theme_xml() {
        let temp = TempDir::new().unwrap();
        let base = temp
            .path()
            .join("app")
            .join("design")
            .join("frontend")
            .join("Vendor");

        // Directory with theme.xml
        let with_xml = base.join("valid");
        fs::create_dir_all(&with_xml).unwrap();
        fs::write(
            with_xml.join("theme.xml"),
            r#"<theme><title>V</title></theme>"#,
        )
        .unwrap();

        // Directory without theme.xml
        let without_xml = base.join("invalid");
        fs::create_dir_all(&without_xml).unwrap();

        let result = discover_themes(temp.path(), Area::Frontend).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "valid");
    }

    #[test]
    fn test_discover_themes_adminhtml_area() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp
            .path()
            .join("app")
            .join("design")
            .join("adminhtml")
            .join("Admin")
            .join("theme");
        fs::create_dir_all(&theme_path).unwrap();

        fs::write(
            theme_path.join("theme.xml"),
            r#"<theme><title>A</title></theme>"#,
        )
        .unwrap();

        let result = discover_themes(temp.path(), Area::Adminhtml).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].area, Area::Adminhtml);
    }

    // ==================== scan_theme_web_sources tests ====================

    #[test]
    fn test_scan_theme_web_sources_exists() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        let web_path = theme_path.join("web");
        fs::create_dir_all(&web_path).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = scan_theme_web_sources(&theme);

        assert_eq!(sources.len(), 1);
        match &sources[0] {
            FileSource::ThemeWeb { theme, path } => {
                assert_eq!(theme, "Test/theme");
                assert_eq!(path, &web_path);
            }
            _ => panic!("Expected ThemeWeb source"),
        }
    }

    #[test]
    fn test_scan_theme_web_sources_not_exists() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        fs::create_dir_all(&theme_path).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = scan_theme_web_sources(&theme);
        assert!(sources.is_empty());
    }

    // ==================== scan_library_sources tests ====================

    #[test]
    fn test_scan_library_sources_exists() {
        let temp = TempDir::new().unwrap();
        let lib_web = temp.path().join("lib").join("web");
        fs::create_dir_all(&lib_web).unwrap();

        let sources = scan_library_sources(temp.path());

        assert_eq!(sources.len(), 1);
        match &sources[0] {
            FileSource::Library { path } => {
                assert_eq!(path, &lib_web);
            }
            _ => panic!("Expected Library source"),
        }
    }

    #[test]
    fn test_scan_library_sources_not_exists() {
        let temp = TempDir::new().unwrap();
        let sources = scan_library_sources(temp.path());
        assert!(sources.is_empty());
    }

    // ==================== scan_vendor_module_sources tests ====================

    #[test]
    fn test_scan_vendor_module_sources_no_vendor() {
        let temp = TempDir::new().unwrap();
        let sources = scan_vendor_module_sources(temp.path(), Area::Frontend);
        assert!(sources.is_empty());
    }

    #[test]
    fn test_scan_vendor_module_sources_standard_path() {
        let temp = TempDir::new().unwrap();
        let module_path = temp
            .path()
            .join("vendor")
            .join("vendorname")
            .join("module-name");
        let web_path = module_path.join("view").join("frontend").join("web");
        let etc_path = module_path.join("etc");
        fs::create_dir_all(&web_path).unwrap();
        fs::create_dir_all(&etc_path).unwrap();

        fs::write(
            etc_path.join("module.xml"),
            r#"<config><module name="Vendor_Module"/></config>"#,
        )
        .unwrap();

        let sources = scan_vendor_module_sources(temp.path(), Area::Frontend);

        assert_eq!(sources.len(), 1);
        match &sources[0] {
            FileSource::VendorModule { module, path } => {
                assert_eq!(module, "Vendor_Module");
                assert_eq!(path, &web_path);
            }
            _ => panic!("Expected VendorModule source"),
        }
    }

    #[test]
    fn test_scan_vendor_module_sources_base_area() {
        let temp = TempDir::new().unwrap();
        let module_path = temp
            .path()
            .join("vendor")
            .join("vendorname")
            .join("module-name");
        let base_web_path = module_path.join("view").join("base").join("web");
        let etc_path = module_path.join("etc");
        fs::create_dir_all(&base_web_path).unwrap();
        fs::create_dir_all(&etc_path).unwrap();

        fs::write(
            etc_path.join("module.xml"),
            r#"<config><module name="Vendor_BaseModule"/></config>"#,
        )
        .unwrap();

        let sources = scan_vendor_module_sources(temp.path(), Area::Frontend);

        assert_eq!(sources.len(), 1);
        match &sources[0] {
            FileSource::VendorModule { module, .. } => {
                assert_eq!(module, "Vendor_BaseModule");
            }
            _ => panic!("Expected VendorModule source"),
        }
    }

    #[test]
    fn test_scan_vendor_module_sources_hyva_style() {
        let temp = TempDir::new().unwrap();
        let module_path = temp.path().join("vendor").join("hyva").join("module-name");
        let web_path = module_path
            .join("src")
            .join("view")
            .join("frontend")
            .join("web");
        let etc_path = module_path.join("src").join("etc");
        fs::create_dir_all(&web_path).unwrap();
        fs::create_dir_all(&etc_path).unwrap();

        fs::write(
            etc_path.join("module.xml"),
            r#"<config><module name="Hyva_Module"/></config>"#,
        )
        .unwrap();

        let sources = scan_vendor_module_sources(temp.path(), Area::Frontend);

        assert_eq!(sources.len(), 1);
    }

    // ==================== scan_theme_module_overrides tests ====================

    #[test]
    fn test_scan_theme_module_overrides_with_modules() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        let override_path = theme_path.join("Magento_Catalog").join("web");
        fs::create_dir_all(&override_path).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = scan_theme_module_overrides(&theme);

        assert_eq!(sources.len(), 1);
        match &sources[0] {
            FileSource::ThemeModuleOverride {
                theme,
                module,
                path,
            } => {
                assert_eq!(theme, "Test/theme");
                assert_eq!(module, "Magento_Catalog");
                assert_eq!(path, &override_path);
            }
            _ => panic!("Expected ThemeModuleOverride source"),
        }
    }

    #[test]
    fn test_scan_theme_module_overrides_skips_non_module_dirs() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");

        // web directory (no underscore)
        fs::create_dir_all(theme_path.join("web")).unwrap();
        // media directory (no underscore)
        fs::create_dir_all(theme_path.join("media")).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = scan_theme_module_overrides(&theme);
        assert!(sources.is_empty());
    }

    #[test]
    fn test_scan_theme_module_overrides_skips_without_web() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");

        // Module dir without web subdirectory
        fs::create_dir_all(theme_path.join("Magento_Catalog").join("templates")).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = scan_theme_module_overrides(&theme);
        assert!(sources.is_empty());
    }

    // ==================== collect_file_sources tests ====================

    #[test]
    fn test_collect_file_sources_priority_order() {
        let temp = TempDir::new().unwrap();

        // Create theme with web and module override
        let theme_path = temp.path().join("theme");
        fs::create_dir_all(theme_path.join("web")).unwrap();
        fs::create_dir_all(theme_path.join("Magento_Catalog").join("web")).unwrap();

        // Create lib/web
        fs::create_dir_all(temp.path().join("lib").join("web")).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = collect_file_sources(&theme, &[], temp.path());

        // Should have: module override, theme web, library
        assert_eq!(sources.len(), 3);

        // Verify order: module override first
        assert!(matches!(
            &sources[0],
            FileSource::ThemeModuleOverride { .. }
        ));
        assert!(matches!(&sources[1], FileSource::ThemeWeb { .. }));
        assert!(matches!(&sources[2], FileSource::Library { .. }));
    }

    #[test]
    fn test_collect_file_sources_with_parent() {
        let temp = TempDir::new().unwrap();

        // Child theme
        let child_path = temp.path().join("child");
        fs::create_dir_all(child_path.join("web")).unwrap();

        // Parent theme
        let parent_path = temp.path().join("parent");
        fs::create_dir_all(parent_path.join("web")).unwrap();

        let child = Theme {
            vendor: "Test".to_string(),
            name: "child".to_string(),
            area: Area::Frontend,
            path: child_path,
            parent: Some(ThemeCode::from("Test/parent")),
            theme_type: ThemeType::Hyva,
        };

        let parent = Theme {
            vendor: "Test".to_string(),
            name: "parent".to_string(),
            area: Area::Frontend,
            path: parent_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = collect_file_sources(&child, &[&parent], temp.path());

        // Child web, then parent web
        assert_eq!(sources.len(), 2);

        match &sources[0] {
            FileSource::ThemeWeb { theme, .. } => assert_eq!(theme, "Test/child"),
            _ => panic!("Expected child ThemeWeb first"),
        }
        match &sources[1] {
            FileSource::ThemeWeb { theme, .. } => assert_eq!(theme, "Test/parent"),
            _ => panic!("Expected parent ThemeWeb second"),
        }
    }

    #[test]
    fn test_collect_file_sources_empty() {
        let temp = TempDir::new().unwrap();
        let theme_path = temp.path().join("theme");
        fs::create_dir_all(&theme_path).unwrap();

        let theme = Theme {
            vendor: "Test".to_string(),
            name: "theme".to_string(),
            area: Area::Frontend,
            path: theme_path,
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let sources = collect_file_sources(&theme, &[], temp.path());
        assert!(sources.is_empty());
    }
}
