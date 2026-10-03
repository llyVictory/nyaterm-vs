use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchEngine {
    pub name: String,
    pub url_template: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_show_in_menu")]
    pub show_in_menu: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSettings {
    #[serde(default = "default_custom_engines")]
    pub custom_engines: Vec<SearchEngine>,
}

fn default_custom_engines() -> Vec<SearchEngine> {
    vec![
        SearchEngine {
            name: "Google".to_string(),
            url_template: "https://www.google.com/search?q=%s".to_string(),
            icon: Some("google".to_string()),
            show_in_menu: true,
        },
        SearchEngine {
            name: "Bing".to_string(),
            url_template: "https://www.bing.com/search?q=%s".to_string(),
            icon: Some("bing".to_string()),
            show_in_menu: true,
        },
        SearchEngine {
            name: "DuckDuckGo".to_string(),
            url_template: "https://duckduckgo.com/?q=%s".to_string(),
            icon: Some("duckduckgo".to_string()),
            show_in_menu: true,
        },
    ]
}

fn default_show_in_menu() -> bool {
    true
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            custom_engines: default_custom_engines(),
        }
    }
}
