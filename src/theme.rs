//! Theme, locale, and area types for Magento 2 static deployment.
//!
//! Provides type-safe wrappers for theme codes, locale codes, and areas
//! with validation and efficient string interning using `Arc<str>`.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Theme code in "Vendor/name" format (e.g., "Hyva/default").
/// Newtype wrapper for type safety and validation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThemeCode(Arc<str>);

impl ThemeCode {
    /// Create a new ThemeCode from vendor and name
    pub fn new(vendor: &str, name: &str) -> Self {
        Self(Arc::from(format!("{}/{}", vendor, name)))
    }

    /// Parse a ThemeCode from "Vendor/name" format
    /// Returns None if format is invalid
    pub fn parse(s: &str) -> Option<Self> {
        if s.contains('/') && s.split('/').count() == 2 {
            Some(Self(Arc::from(s)))
        } else {
            None
        }
    }

    /// Get the inner string reference
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the vendor part (before the slash)
    #[inline]
    pub fn vendor(&self) -> &str {
        self.0.split('/').next().unwrap_or("")
    }

    /// Get the name part (after the slash)
    #[inline]
    pub fn name(&self) -> &str {
        self.0.split('/').nth(1).unwrap_or("")
    }
}

impl fmt::Display for ThemeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ThemeCode {
    fn from(s: &str) -> Self {
        Self(Arc::from(s))
    }
}

/// Locale code (e.g., "en_US", "nl_NL")
/// Newtype wrapper for type safety
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocaleCode(Arc<str>);

impl LocaleCode {
    /// Create a new LocaleCode without validation (for backwards compatibility)
    pub fn new(s: &str) -> Self {
        Self(Arc::from(s))
    }

    /// Create a validated LocaleCode, returning error for invalid format
    /// Format must be xx_YY (e.g., en_US, nl_NL, de_DE)
    pub fn validated(s: &str) -> Result<Self, String> {
        if Self::validate_format(s) {
            Ok(Self(Arc::from(s)))
        } else {
            Err(format!(
                "invalid locale format '{}': expected xx_YY (e.g., en_US)",
                s
            ))
        }
    }

    /// Validate locale format: xx_YY where xx is lowercase and YY is uppercase
    #[inline]
    fn validate_format(s: &str) -> bool {
        let bytes = s.as_bytes();
        bytes.len() == 5
            && bytes[2] == b'_'
            && bytes[0].is_ascii_lowercase()
            && bytes[1].is_ascii_lowercase()
            && bytes[3].is_ascii_uppercase()
            && bytes[4].is_ascii_uppercase()
    }

    /// Get the inner string reference
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a valid locale format (xx_YY)
    #[inline]
    pub fn is_valid_format(&self) -> bool {
        Self::validate_format(&self.0)
    }
}

impl fmt::Display for LocaleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for LocaleCode {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for LocaleCode {
    fn from(s: String) -> Self {
        Self(Arc::from(s.as_str()))
    }
}

/// Magento area (frontend or admin)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Area {
    Frontend,
    Adminhtml,
}

impl Area {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Area::Frontend => "frontend",
            Area::Adminhtml => "adminhtml",
        }
    }

    #[inline]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "frontend" => Some(Area::Frontend),
            "adminhtml" => Some(Area::Adminhtml),
            _ => None,
        }
    }
}

/// Theme type determines deployment strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeType {
    /// Hyva theme: fast file copy, no LESS compilation
    Hyva,
    /// Luma theme: delegate to bin/magento for LESS/RequireJS
    Luma,
}

/// A Magento theme with its metadata and inheritance chain
#[derive(Debug, Clone)]
pub struct Theme {
    /// Vendor name (e.g., "Hyva", "Magento")
    pub vendor: String,
    /// Theme name (e.g., "reset", "luma")
    pub name: String,
    /// Area: "frontend" or "adminhtml"
    pub area: Area,
    /// Full path to theme directory
    pub path: PathBuf,
    /// Parent theme in inheritance chain (None if root)
    pub parent: Option<ThemeCode>,
    /// Theme type determines deployment strategy
    pub theme_type: ThemeType,
}

impl Theme {
    /// Get full theme identifier as ThemeCode
    pub fn code(&self) -> ThemeCode {
        ThemeCode::new(&self.vendor, &self.name)
    }

    /// Get full theme identifier: Vendor/name (for compatibility)
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.vendor, self.name)
    }
}

/// Parse parent theme from theme.xml content
pub fn parse_theme_xml(xml: &str) -> Option<ThemeCode> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut in_parent = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"parent" => {
                in_parent = true;
            }
            Ok(Event::Text(e)) if in_parent => {
                return e.unescape().ok().and_then(|s| ThemeCode::parse(s.trim()));
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"parent" => {
                in_parent = false;
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

/// Determine if theme is Hyva based on theme.xml content and parent chain
pub fn is_hyva_theme(theme_xml_content: &str, parent_chain: &[String]) -> bool {
    theme_xml_content.contains("Hyva_Theme") || parent_chain.iter().any(|p| p.starts_with("Hyva/"))
}

/// Determine theme type from xml content and parent chain
pub fn detect_theme_type(theme_xml_content: &str, parent_chain: &[String]) -> ThemeType {
    if is_hyva_theme(theme_xml_content, parent_chain) {
        ThemeType::Hyva
    } else {
        ThemeType::Luma
    }
}

/// Resolve full parent chain for a theme (child first, root last)
pub fn resolve_parent_chain<'a>(theme: &Theme, all_themes: &'a [Theme]) -> Vec<&'a Theme> {
    // Pre-allocate for typical parent chain depth (2-4 themes)
    let mut chain = Vec::with_capacity(4);
    let mut current_parent = theme.parent.as_ref();

    while let Some(parent_code) = current_parent {
        if let Some(parent_theme) = all_themes
            .iter()
            .find(|t| t.area == theme.area && t.code() == *parent_code)
        {
            chain.push(parent_theme);
            current_parent = parent_theme.parent.as_ref();
        } else {
            break;
        }
    }

    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    // ThemeCode tests
    #[test]
    fn test_theme_code_new() {
        let code = ThemeCode::new("Hyva", "default");
        assert_eq!(code.as_str(), "Hyva/default");
        assert_eq!(code.vendor(), "Hyva");
        assert_eq!(code.name(), "default");
    }

    #[test]
    fn test_theme_code_parse_valid() {
        let parsed = ThemeCode::parse("Magento/blank");
        assert!(parsed.is_some());
        let code = parsed.unwrap();
        assert_eq!(code.vendor(), "Magento");
        assert_eq!(code.name(), "blank");
    }

    #[test]
    fn test_theme_code_parse_invalid() {
        assert!(ThemeCode::parse("invalid").is_none());
        assert!(ThemeCode::parse("too/many/slashes").is_none());
        assert!(ThemeCode::parse("").is_none());
    }

    #[test]
    fn test_theme_code_display() {
        let code = ThemeCode::new("Hyva", "reset");
        assert_eq!(format!("{}", code), "Hyva/reset");
    }

    #[test]
    fn test_theme_code_from_str() {
        let code: ThemeCode = "Magento/luma".into();
        assert_eq!(code.as_str(), "Magento/luma");
    }

    #[test]
    fn test_theme_code_equality() {
        let code1 = ThemeCode::new("Hyva", "default");
        let code2 = ThemeCode::parse("Hyva/default").unwrap();
        assert_eq!(code1, code2);
    }

    // LocaleCode tests
    #[test]
    fn test_locale_code_new() {
        let locale = LocaleCode::new("en_US");
        assert_eq!(locale.as_str(), "en_US");
    }

    #[test]
    fn test_locale_code_validated_success() {
        let result = LocaleCode::validated("en_US");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "en_US");

        let result = LocaleCode::validated("nl_NL");
        assert!(result.is_ok());

        let result = LocaleCode::validated("de_DE");
        assert!(result.is_ok());
    }

    #[test]
    fn test_locale_code_validated_failure() {
        // Wrong length
        assert!(LocaleCode::validated("english").is_err());
        assert!(LocaleCode::validated("en").is_err());
        assert!(LocaleCode::validated("").is_err());

        // Wrong format
        assert!(LocaleCode::validated("EN_US").is_err()); // uppercase language
        assert!(LocaleCode::validated("en_us").is_err()); // lowercase country
        assert!(LocaleCode::validated("enUS_").is_err()); // wrong position
    }

    #[test]
    fn test_locale_code_is_valid_format() {
        assert!(LocaleCode::new("en_US").is_valid_format());
        assert!(LocaleCode::new("nl_NL").is_valid_format());
        assert!(!LocaleCode::new("invalid").is_valid_format());
        assert!(!LocaleCode::new("EN_US").is_valid_format());
    }

    #[test]
    fn test_locale_code_display() {
        let locale = LocaleCode::new("de_DE");
        assert_eq!(format!("{}", locale), "de_DE");
    }

    #[test]
    fn test_locale_code_from_str() {
        let locale: LocaleCode = "fr_FR".into();
        assert_eq!(locale.as_str(), "fr_FR");
    }

    #[test]
    fn test_locale_code_from_string() {
        let locale: LocaleCode = String::from("it_IT").into();
        assert_eq!(locale.as_str(), "it_IT");
    }

    // Area tests
    #[test]
    fn test_area_as_str() {
        assert_eq!(Area::Frontend.as_str(), "frontend");
        assert_eq!(Area::Adminhtml.as_str(), "adminhtml");
    }

    #[test]
    fn test_area_parse() {
        assert_eq!(Area::parse("frontend"), Some(Area::Frontend));
        assert_eq!(Area::parse("adminhtml"), Some(Area::Adminhtml));
        assert_eq!(Area::parse("invalid"), None);
        assert_eq!(Area::parse(""), None);
    }

    // Theme tests
    #[test]
    fn test_theme_code_method() {
        let theme = Theme {
            vendor: "Hyva".to_string(),
            name: "default".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/app/design/frontend/Hyva/default"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };
        assert_eq!(theme.code().as_str(), "Hyva/default");
    }

    #[test]
    fn test_theme_full_name() {
        let theme = Theme {
            vendor: "Magento".to_string(),
            name: "blank".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/app/design/frontend/Magento/blank"),
            parent: None,
            theme_type: ThemeType::Luma,
        };
        assert_eq!(theme.full_name(), "Magento/blank");
    }

    // XML parsing tests
    #[test]
    fn test_parse_theme_xml_with_parent() {
        let xml = r#"<?xml version="1.0"?>
<theme xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <title>My Theme</title>
    <parent>Hyva/reset</parent>
</theme>"#;

        let result = parse_theme_xml(xml);
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_str(), "Hyva/reset");
    }

    #[test]
    fn test_parse_theme_xml_no_parent() {
        let xml = r#"<?xml version="1.0"?>
<theme xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <title>Root Theme</title>
</theme>"#;

        let result = parse_theme_xml(xml);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_theme_xml_empty_parent() {
        let xml = r#"<theme><parent></parent></theme>"#;
        let result = parse_theme_xml(xml);
        assert!(result.is_none()); // Empty parent should not parse
    }

    #[test]
    fn test_parse_theme_xml_malformed() {
        let xml = "not xml at all";
        let result = parse_theme_xml(xml);
        assert!(result.is_none());
    }

    // Theme type detection tests
    #[test]
    fn test_is_hyva_theme_by_content() {
        let xml_with_hyva = "<module name=\"Hyva_Theme\"/>";
        assert!(is_hyva_theme(xml_with_hyva, &[]));
    }

    #[test]
    fn test_is_hyva_theme_by_parent() {
        let xml_without = "<theme/>";
        assert!(is_hyva_theme(xml_without, &["Hyva/reset".to_string()]));
        assert!(is_hyva_theme(xml_without, &["Hyva/default".to_string()]));
    }

    #[test]
    fn test_is_not_hyva_theme() {
        let xml = "<theme/>";
        assert!(!is_hyva_theme(xml, &[]));
        assert!(!is_hyva_theme(xml, &["Magento/blank".to_string()]));
    }

    #[test]
    fn test_detect_theme_type_hyva() {
        let xml = "<module name=\"Hyva_Theme\"/>";
        assert_eq!(detect_theme_type(xml, &[]), ThemeType::Hyva);
    }

    #[test]
    fn test_detect_theme_type_luma() {
        let xml = "<theme/>";
        assert_eq!(detect_theme_type(xml, &[]), ThemeType::Luma);
    }

    // Parent chain resolution tests
    #[test]
    fn test_resolve_parent_chain_no_parent() {
        let theme = Theme {
            vendor: "Hyva".to_string(),
            name: "reset".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/theme"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let chain = resolve_parent_chain(&theme, &[]);
        assert!(chain.is_empty());
    }

    #[test]
    fn test_resolve_parent_chain_with_parent() {
        let parent_theme = Theme {
            vendor: "Hyva".to_string(),
            name: "reset".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/parent"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let child_theme = Theme {
            vendor: "Custom".to_string(),
            name: "child".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/child"),
            parent: Some(ThemeCode::parse("Hyva/reset").unwrap()),
            theme_type: ThemeType::Hyva,
        };

        let all_themes = vec![parent_theme];
        let chain = resolve_parent_chain(&child_theme, &all_themes);

        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].full_name(), "Hyva/reset");
    }

    #[test]
    fn test_resolve_parent_chain_missing_parent() {
        let theme = Theme {
            vendor: "Custom".to_string(),
            name: "orphan".to_string(),
            area: Area::Frontend,
            path: PathBuf::from("/orphan"),
            parent: Some(ThemeCode::parse("Missing/parent").unwrap()),
            theme_type: ThemeType::Hyva,
        };

        let chain = resolve_parent_chain(&theme, &[]);
        assert!(chain.is_empty()); // Parent not found
    }

    #[test]
    fn test_resolve_parent_chain_different_area() {
        let parent_theme = Theme {
            vendor: "Hyva".to_string(),
            name: "reset".to_string(),
            area: Area::Adminhtml, // Different area
            path: PathBuf::from("/parent"),
            parent: None,
            theme_type: ThemeType::Hyva,
        };

        let child_theme = Theme {
            vendor: "Custom".to_string(),
            name: "child".to_string(),
            area: Area::Frontend, // Different area
            path: PathBuf::from("/child"),
            parent: Some(ThemeCode::parse("Hyva/reset").unwrap()),
            theme_type: ThemeType::Hyva,
        };

        let all_themes = vec![parent_theme];
        let chain = resolve_parent_chain(&child_theme, &all_themes);

        assert!(chain.is_empty()); // Different area, parent not matched
    }
}
