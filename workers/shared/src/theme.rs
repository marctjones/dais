use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub light: ColorScheme,
    pub dark: ColorScheme,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorScheme {
    // Backgrounds (60% - dominant)
    pub bg_primary: String,      // Main background
    pub bg_secondary: String,    // Cards, panels

    // Text colors
    pub text_primary: String,    // Main text
    pub text_secondary: String,  // Muted text, metadata

    // Accent colors (10%)
    pub accent_primary: String,  // Main brand color
    pub accent_hover: String,    // Hover states

    // Borders and dividers
    pub border: String,

    // Shadows
    pub shadow: String,
}

impl Theme {
    /// Get theme by name from environment or default to "dais"
    pub fn from_name(name: &str) -> Self {
        match name {
            "dais-light" => Self::dais_light(),
            "cat" => Self::cat(),
            "cat-light" => Self::cat_light(),
            _ => Self::dais(), // default
        }
    }

    /// Current sophisticated teal theme
    pub fn dais() -> Self {
        Theme {
            name: "dais".to_string(),
            light: ColorScheme {
                bg_primary: "#FAFAF9".to_string(),
                bg_secondary: "#F5F5F4".to_string(),
                text_primary: "#1C1917".to_string(),
                text_secondary: "#57534E".to_string(),
                accent_primary: "#14B8A6".to_string(),
                accent_hover: "#0F766E".to_string(),
                border: "#E7E5E4".to_string(),
                shadow: "0 1px 3px rgba(0, 0, 0, 0.1)".to_string(),
            },
            dark: ColorScheme {
                bg_primary: "#1C1917".to_string(),
                bg_secondary: "#292524".to_string(),
                text_primary: "#FAFAF9".to_string(),
                text_secondary: "#D6D3D1".to_string(),
                accent_primary: "#2DD4BF".to_string(),
                accent_hover: "#5EEAD4".to_string(),
                border: "#44403C".to_string(),
                shadow: "0 1px 3px rgba(0, 0, 0, 0.3)".to_string(),
            },
        }
    }

    /// Lighter, softer teal variant
    pub fn dais_light() -> Self {
        Theme {
            name: "dais-light".to_string(),
            light: ColorScheme {
                bg_primary: "#FFFFFF".to_string(),
                bg_secondary: "#F0FDFA".to_string(),  // Very light teal tint
                text_primary: "#0F172A".to_string(),
                text_secondary: "#64748B".to_string(),
                accent_primary: "#5EEAD4".to_string(),  // Lighter teal
                accent_hover: "#2DD4BF".to_string(),
                border: "#E2E8F0".to_string(),
                shadow: "0 1px 2px rgba(0, 0, 0, 0.05)".to_string(),
            },
            dark: ColorScheme {
                bg_primary: "#0F172A".to_string(),
                bg_secondary: "#1E293B".to_string(),
                text_primary: "#F1F5F9".to_string(),
                text_secondary: "#CBD5E1".to_string(),
                accent_primary: "#5EEAD4".to_string(),
                accent_hover: "#99F6E4".to_string(),
                border: "#334155".to_string(),
                shadow: "0 1px 3px rgba(0, 0, 0, 0.4)".to_string(),
            },
        }
    }

    /// Warm, playful theme inspired by pet care/cat hotel aesthetics
    pub fn cat() -> Self {
        Theme {
            name: "cat".to_string(),
            light: ColorScheme {
                bg_primary: "#FFF8F3".to_string(),      // Warm cream
                bg_secondary: "#FFF1E6".to_string(),    // Soft peach
                text_primary: "#78350F".to_string(),    // Warm brown
                text_secondary: "#92400E".to_string(),  // Medium brown
                accent_primary: "#FB923C".to_string(),  // Warm orange
                accent_hover: "#F97316".to_string(),    // Bright orange
                border: "#FED7AA".to_string(),          // Light orange border
                shadow: "0 2px 4px rgba(251, 146, 60, 0.1)".to_string(),
            },
            dark: ColorScheme {
                bg_primary: "#1C1917".to_string(),      // Dark warm gray
                bg_secondary: "#292524".to_string(),    // Lighter warm gray
                text_primary: "#FFF8F3".to_string(),    // Warm off-white
                text_secondary: "#D6D3D1".to_string(),  // Warm light gray
                accent_primary: "#FDBA74".to_string(),  // Soft orange
                accent_hover: "#FCD34D".to_string(),    // Golden yellow
                border: "#44403C".to_string(),
                shadow: "0 2px 4px rgba(253, 186, 116, 0.15)".to_string(),
            },
        }
    }

    /// Lighter, airier version of cat theme with white backgrounds
    pub fn cat_light() -> Self {
        Theme {
            name: "cat-light".to_string(),
            light: ColorScheme {
                bg_primary: "#FFFFFF".to_string(),      // Pure white
                bg_secondary: "#FFFAF5".to_string(),    // Very light warm white
                text_primary: "#92400E".to_string(),    // Medium brown
                text_secondary: "#B45309".to_string(),  // Lighter brown
                accent_primary: "#FDBA74".to_string(),  // Soft peach
                accent_hover: "#FB923C".to_string(),    // Medium orange
                border: "#FEE2C1".to_string(),          // Very light peach border
                shadow: "0 1px 3px rgba(251, 146, 60, 0.08)".to_string(),
            },
            dark: ColorScheme {
                bg_primary: "#1C1917".to_string(),      // Dark warm gray
                bg_secondary: "#292524".to_string(),    // Lighter warm gray
                text_primary: "#FFFAF5".to_string(),    // Warm white
                text_secondary: "#E7E5E4".to_string(),  // Light warm gray
                accent_primary: "#FCD34D".to_string(),  // Golden yellow
                accent_hover: "#FDE68A".to_string(),    // Lighter yellow
                border: "#44403C".to_string(),
                shadow: "0 2px 4px rgba(252, 211, 77, 0.12)".to_string(),
            },
        }
    }

    /// Generate CSS for light and dark modes
    pub fn generate_css(&self) -> String {
        format!(r#"
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Helvetica Neue', Arial, sans-serif;
            background: {};
            color: {};
            line-height: 1.6;
            padding: 20px;
        }}
        .container {{
            max-width: 600px;
            margin: 40px auto;
        }}
        @media (prefers-color-scheme: dark) {{
            body {{
                background: {};
                color: {};
            }}
        }}
        "#,
            self.light.bg_primary,
            self.light.text_primary,
            self.dark.bg_primary,
            self.dark.text_primary
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_from_name() {
        let theme = Theme::from_name("dais");
        assert_eq!(theme.name, "dais");
        assert_eq!(theme.light.accent_primary, "#14B8A6");
    }

    #[test]
    fn test_theme_dais_light() {
        let theme = Theme::from_name("dais-light");
        assert_eq!(theme.name, "dais-light");
        assert_eq!(theme.light.bg_primary, "#FFFFFF");
    }

    #[test]
    fn test_theme_cat() {
        let theme = Theme::from_name("cat");
        assert_eq!(theme.name, "cat");
        assert_eq!(theme.light.accent_primary, "#FB923C");
    }

    #[test]
    fn test_theme_cat_light() {
        let theme = Theme::from_name("cat-light");
        assert_eq!(theme.name, "cat-light");
        assert_eq!(theme.light.bg_primary, "#FFFFFF");
        assert_eq!(theme.light.accent_primary, "#FDBA74");
    }

    #[test]
    fn test_default_theme() {
        let theme = Theme::from_name("unknown");
        assert_eq!(theme.name, "dais");
    }
}
