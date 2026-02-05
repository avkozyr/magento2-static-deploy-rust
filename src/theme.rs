use quick_xml::events::Event;
use quick_xml::Reader;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Theme code in "Vendor/name" format (e.g., "Hyva/default")
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
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the vendor part (before the slash)
    pub fn vendor(&self) -> &str {
        self.0.split('/').next().unwrap_or("")
    }

    /// Get the name part (after the slash)
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
    /// Create a new LocaleCode
    pub fn new(s: &str) -> Self {
        Self(Arc::from(s))
    }

    /// Get the inner string reference
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a valid locale format (xx_YY)
    pub fn is_valid_format(&self) -> bool {
        let parts: Vec<&str> = self.0.split('_').collect();
        parts.len() == 2 && parts[0].len() == 2 && parts[1].len() == 2
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Frontend,
    Adminhtml,
}

impl Area {
    pub fn as_str(&self) -> &'static str {
        match self {
            Area::Frontend => "frontend",
            Area::Adminhtml => "adminhtml",
        }
    }

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
                return e
                    .unescape()
                    .ok()
                    .and_then(|s| ThemeCode::parse(s.trim()));
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
    let mut chain = Vec::new();
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

    #[test]
    fn test_parse_theme_xml() {
        let xml = r#"<?xml version="1.0"?>
<theme xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <title>My Theme</title>
    <parent>Hyva/reset</parent>
</theme>"#;

        let result = parse_theme_xml(xml);
        assert!(result.is_some());
        assert_eq!(result.as_ref().map(|c| c.as_str()), Some("Hyva/reset"));
    }

    #[test]
    fn test_theme_code() {
        let code = ThemeCode::new("Hyva", "default");
        assert_eq!(code.as_str(), "Hyva/default");
        assert_eq!(code.vendor(), "Hyva");
        assert_eq!(code.name(), "default");

        let parsed = ThemeCode::parse("Magento/blank");
        assert!(parsed.is_some());
        assert_eq!(parsed.as_ref().map(|c| c.vendor()), Some("Magento"));

        // Invalid formats
        assert!(ThemeCode::parse("invalid").is_none());
        assert!(ThemeCode::parse("too/many/slashes").is_none());
    }

    #[test]
    fn test_locale_code() {
        let locale = LocaleCode::new("en_US");
        assert_eq!(locale.as_str(), "en_US");
        assert!(locale.is_valid_format());

        let invalid = LocaleCode::new("english");
        assert!(!invalid.is_valid_format());
    }

    #[test]
    fn test_is_hyva_theme() {
        let xml_with_hyva = "<module name=\"Hyva_Theme\"/>";
        assert!(is_hyva_theme(xml_with_hyva, &[]));

        let xml_without = "<theme/>";
        assert!(is_hyva_theme(xml_without, &["Hyva/reset".to_string()]));

        assert!(!is_hyva_theme(xml_without, &["Magento/blank".to_string()]));
    }
}
