use regex::Regex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const AUTO_LOGIN_TAIL_CHARS: usize = 2048;
const AUTO_LOGIN_PROMPT_WINDOW_CHARS: usize = 320;

#[derive(Debug, Clone)]
pub struct TelnetAutoLoginConfig {
    pub enabled: bool,
    pub send_wake_enter: bool,
    pub timeout_ms: u64,
    pub username_prompt_regex: Option<String>,
    pub password_prompt_regex: Option<String>,
    pub success_prompt_regex: Option<String>,
    pub failure_prompt_regex: Option<String>,
    pub max_retries: u8,
}

impl Default for TelnetAutoLoginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            send_wake_enter: true,
            timeout_ms: 60_000,
            username_prompt_regex: None,
            password_prompt_regex: None,
            success_prompt_regex: None,
            failure_prompt_regex: None,
            max_retries: 0,
        }
    }
}

impl From<crate::config::TelnetAutoLoginConfig> for TelnetAutoLoginConfig {
    fn from(value: crate::config::TelnetAutoLoginConfig) -> Self {
        Self {
            enabled: value.enabled,
            send_wake_enter: value.send_wake_enter,
            timeout_ms: value.timeout_ms,
            username_prompt_regex: value.username_prompt_regex,
            password_prompt_regex: value.password_prompt_regex,
            success_prompt_regex: value.success_prompt_regex,
            failure_prompt_regex: value.failure_prompt_regex,
            max_retries: value.max_retries,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelnetAutoLoginCredentials {
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelnetAutoLoginAction {
    Send(Vec<u8>),
    Complete,
    Disable,
}

#[derive(Debug)]
pub struct TelnetAutoLogin {
    config: TelnetAutoLoginConfig,
    credentials: TelnetAutoLoginCredentials,
    enter_mode: TelnetEnterMode,
    started_at: Instant,
    tail: String,
    sent_wake: bool,
    sent_username: bool,
    sent_password: bool,
    disabled: bool,
    completed: bool,
    retries: u8,
    username_regex: Option<Regex>,
    password_regex: Option<Regex>,
    success_regex: Option<Regex>,
    failure_regex: Option<Regex>,
}

impl TelnetAutoLogin {
    pub fn new(
        config: TelnetAutoLoginConfig,
        credentials: TelnetAutoLoginCredentials,
        enter_mode: TelnetEnterMode,
        started_at: Instant,
    ) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        if credentials.username.trim().is_empty()
            && credentials.password.as_deref().is_none_or(str::is_empty)
        {
            return None;
        }

        Some(Self {
            username_regex: compile_optional_regex(config.username_prompt_regex.as_deref()),
            password_regex: compile_optional_regex(config.password_prompt_regex.as_deref()),
            success_regex: compile_optional_regex(config.success_prompt_regex.as_deref()),
            failure_regex: compile_optional_regex(config.failure_prompt_regex.as_deref()),
            config,
            credentials,
            enter_mode,
            started_at,
            tail: String::new(),
            sent_wake: false,
            sent_username: false,
            sent_password: false,
            disabled: false,
            completed: false,
            retries: 0,
        })
    }

    pub fn handle_text(&mut self, text: &str, now: Instant) -> Vec<TelnetAutoLoginAction> {
        if self.disabled || self.completed {
            return Vec::new();
        }

        if let Some(action) = self.tick(now) {
            return vec![action];
        }

        self.push_tail(text);
        let clean = strip_ansi_escapes::strip_str(&self.tail);
        let clean_input = strip_ansi_escapes::strip_str(text).replace('\r', "\n");
        let normalized = clean.replace('\r', "\n");
        let window = last_chars(&normalized, AUTO_LOGIN_PROMPT_WINDOW_CHARS);
        let last_line = last_non_empty_line(&normalized);
        let prompts = prompt_candidates(&window, &clean_input);

        if self.matches_failure(&window, &last_line) {
            if self.retries < self.config.max_retries {
                self.retries += 1;
                self.sent_username = false;
                self.sent_password = false;
                self.tail.clear();
                return Vec::new();
            }

            self.disabled = true;
            return vec![TelnetAutoLoginAction::Disable];
        }

        let mut actions = Vec::new();
        if self.config.send_wake_enter && !self.sent_wake && self.matches_wake_prompt(&window) {
            self.sent_wake = true;
            actions.push(TelnetAutoLoginAction::Send(line_bytes("", self.enter_mode)));
        }

        if !self.sent_username
            && !self.credentials.username.trim().is_empty()
            && self.matches_username_prompt(&prompts, &last_line)
        {
            self.sent_username = true;
            actions.push(TelnetAutoLoginAction::Send(line_bytes(
                &self.credentials.username,
                self.enter_mode,
            )));
        }

        if !self.sent_password
            && self
                .credentials
                .password
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            && self.matches_password_prompt(&prompts)
        {
            self.sent_password = true;
            actions.push(TelnetAutoLoginAction::Send(line_bytes(
                self.credentials.password.as_deref().unwrap_or_default(),
                self.enter_mode,
            )));
        }

        if (self.sent_username || self.sent_password) && self.matches_success(&last_line) {
            self.completed = true;
            actions.push(TelnetAutoLoginAction::Complete);
        }

        actions
    }

    pub fn handle_user_input(&mut self, automated: bool) -> Option<TelnetAutoLoginAction> {
        if automated || self.disabled || self.completed {
            return None;
        }
        self.disabled = true;
        Some(TelnetAutoLoginAction::Disable)
    }

    pub fn tick(&mut self, now: Instant) -> Option<TelnetAutoLoginAction> {
        if !self.disabled
            && !self.completed
            && now.duration_since(self.started_at) > Duration::from_millis(self.config.timeout_ms)
        {
            self.disabled = true;
            Some(TelnetAutoLoginAction::Disable)
        } else {
            None
        }
    }

    fn push_tail(&mut self, text: &str) {
        self.tail.push_str(text);
        self.tail = last_chars(&self.tail, AUTO_LOGIN_TAIL_CHARS);
    }

    fn matches_wake_prompt(&self, text: &str) -> bool {
        default_wake_regex().is_match(text)
    }

    fn matches_username_prompt(&self, prompts: &[String], last_line: &str) -> bool {
        if last_login_regex().is_match(last_line) {
            return false;
        }

        prompts.iter().any(|prompt| {
            self.username_regex.as_ref().map_or_else(
                || default_username_regex().is_match(prompt),
                |regex| regex.is_match(prompt),
            )
        })
    }

    fn matches_password_prompt(&self, prompts: &[String]) -> bool {
        prompts.iter().any(|prompt| {
            self.password_regex.as_ref().map_or_else(
                || default_password_regex().is_match(prompt),
                |regex| regex.is_match(prompt),
            )
        })
    }

    fn matches_success(&self, last_line: &str) -> bool {
        self.success_regex.as_ref().map_or_else(
            || default_success_regex().is_match(last_line),
            |regex| regex.is_match(last_line),
        )
    }

    fn matches_failure(&self, text: &str, last_line: &str) -> bool {
        self.failure_regex.as_ref().map_or_else(
            || {
                default_failure_regex().is_match(text)
                    || default_failure_regex().is_match(last_line)
            },
            |regex| regex.is_match(text) || regex.is_match(last_line),
        )
    }
}

pub fn compile_optional_regex(pattern: Option<&str>) -> Option<Regex> {
    let trimmed = pattern?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Regex::new(trimmed).ok()
}

pub fn line_bytes(value: &str, enter_mode: TelnetEnterMode) -> Vec<u8> {
    let mut data = value.as_bytes().to_vec();
    data.push(b'\r');
    normalize_enter_bytes(&data, enter_mode)
}

pub fn last_chars(value: &str, max_chars: usize) -> String {
    let len = value.chars().count();
    if len <= max_chars {
        return value.to_string();
    }
    value.chars().skip(len - max_chars).collect()
}

pub fn last_non_empty_line(value: &str) -> String {
    value
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn prompt_candidates(window: &str, current_input: &str) -> Vec<String> {
    let mut prompts = Vec::new();
    for source in [window, current_input] {
        for line in source.lines() {
            let prompt = line.trim();
            push_prompt_candidate(&mut prompts, prompt);
            push_prompt_suffix_candidates(&mut prompts, prompt);
        }
    }
    prompts
}

pub fn push_prompt_candidate(prompts: &mut Vec<String>, prompt: &str) {
    if !prompt.is_empty() && !prompts.iter().any(|existing| existing == prompt) {
        prompts.push(prompt.to_string());
    }
}

pub fn push_prompt_suffix_candidates(prompts: &mut Vec<String>, prompt: &str) {
    const KEYWORDS: &[&str] = &[
        "user name",
        "username",
        "login",
        "logon",
        "account",
        "userid",
        "user id",
        "user",
        "password",
        "passwd",
        "passcode",
        "passphrase",
        "pin",
        "用户名",
        "帐号",
        "账号",
        "登录",
        "登入",
        "密码",
        "口令",
    ];

    let lower = prompt.to_lowercase();
    for keyword in KEYWORDS {
        let mut search_start = 0;
        while let Some(offset) = lower[search_start..].find(keyword) {
            let start = search_start + offset;
            push_prompt_candidate(prompts, prompt[start..].trim());
            search_start = start + keyword.len();
            if search_start >= lower.len() {
                break;
            }
        }
    }
}

pub fn default_username_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:[^\r\n:：>]{1,80}\s+)?(?:user\s*name|username|login|logon|account|userid|user\s*id|user|用户名|帐号|账号|登录|登入)\s*[:：>]\s*$",
        )
        .expect("default username prompt regex")
    })
}

pub fn last_login_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)\b(?:last|previous)\s+login\b").expect("last login regex"))
}

pub fn default_password_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[\r\n])\s*(?:input\s+)?(?:password|passwd|passcode|passphrase|pin|密码|口令)\s*[:：>]?\s*$",
        )
        .expect("default password prompt regex")
    })
}

pub fn default_wake_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)(press\s+(?:return|<enter>|\[enter\]|enter|any\s+key))")
            .expect("default wake prompt regex")
    })
}

pub fn default_success_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[$#>]\s*$").expect("default success prompt regex"))
}

pub fn default_failure_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(login\s+incorrect|authentication\s+failed|access\s+denied|密码错误|认证失败)",
        )
        .expect("default failure prompt regex")
    })
}
