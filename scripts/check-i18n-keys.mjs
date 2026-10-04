import { readFileSync } from "node:fs";
function keys(value, prefix = "") {
  return Object.entries(value)
    .flatMap(([key, child]) => {
      const path = prefix ? `${prefix}.${key}` : key;
      return typeof child === "object" && child !== null
        ? keys(child, path)
        : [path];
    })
    .map((key) => key.replace(/_(zero|one|two|few|many|other|plural)$/, ""))
    .filter((key, index, all) => all.indexOf(key) === index)
    .sort();
}
const locales = ["en", "zh-CN", "zh-TW", "ko"];
const reference = keys(
  JSON.parse(readFileSync(`src/i18n/locales/en.json`, "utf8")),
);
for (const locale of locales.slice(1)) {
  const current = keys(
    JSON.parse(readFileSync(`src/i18n/locales/${locale}.json`, "utf8")),
  );
  const missing = reference.filter((key) => !current.includes(key));
  const extra = current.filter((key) => !reference.includes(key));
  if (missing.length || extra.length)
    throw new Error(
      `${locale}: missing ${missing.join(", ")}; extra ${extra.join(", ")}`,
    );
}
console.log(
  `All four locales have the same ${reference.length} translation keys.`,
);
