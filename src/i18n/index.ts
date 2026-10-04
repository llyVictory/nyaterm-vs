import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { runtime } from "@/lib/backend/runtime";
import en from "./locales/en.json";
import ko from "./locales/ko.json";
import zhCN from "./locales/zh-CN.json";
import zhTW from "./locales/zh-TW.json";

const WEB_LANGUAGE_CACHE_KEY = "nyaterm-web-language";
const supportedLanguages = ["en", "zh-CN", "zh-TW", "ko"];
function initialLanguage(): string {
  if (runtime !== "web") return "en";
  try {
    const cached = localStorage.getItem(WEB_LANGUAGE_CACHE_KEY);
    if (cached && supportedLanguages.includes(cached)) return cached;
  } catch {}
  const language = navigator.language.toLowerCase();
  if (language.startsWith("zh")) return /tw|hk|hant/.test(language) ? "zh-TW" : "zh-CN";
  return language.startsWith("ko") ? "ko" : "en";
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    "zh-CN": { translation: zhCN },
    "zh-TW": { translation: zhTW },
    ko: { translation: ko },
  },
  lng: initialLanguage(),
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
});

if (runtime === "web")
  i18n.on("languageChanged", (language) => {
    if (supportedLanguages.includes(language)) {
      try {
        localStorage.setItem(WEB_LANGUAGE_CACHE_KEY, language);
      } catch {}
    }
  });

export const AVAILABLE_LANGUAGES = [
  { id: "en", name: "English" },
  { id: "zh-CN", name: "中文 (简体)" },
  { id: "zh-TW", name: "繁體中文" },
  { id: "ko", name: "한국어" },
];

export default i18n;
