use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSettings {
    #[serde(default = "default_target_language")]
    pub target_language: String,
    #[serde(default)]
    pub deepl_api_key: String,
    #[serde(default)]
    pub baidu_app_id: String,
    #[serde(default)]
    pub baidu_app_key: String,
    #[serde(default)]
    pub ali_app_id: String,
    #[serde(default)]
    pub ali_app_key: String,
    #[serde(default)]
    pub youdao_app_id: String,
    #[serde(default)]
    pub youdao_app_key: String,
}

fn default_target_language() -> String {
    "zh-CN".to_string()
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            target_language: default_target_language(),
            deepl_api_key: String::new(),
            baidu_app_id: String::new(),
            baidu_app_key: String::new(),
            ali_app_id: String::new(),
            ali_app_key: String::new(),
            youdao_app_id: String::new(),
            youdao_app_key: String::new(),
        }
    }
}
