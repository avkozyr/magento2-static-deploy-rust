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

                    let mut sources = Vec::new();

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
    let mut sources = Vec::new();

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
    let mut sources = Vec::new();

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
