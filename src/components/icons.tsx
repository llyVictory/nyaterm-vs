import type { ElementType } from "react";
import type { IconType } from "react-icons";
import { DiBingSmall, DiYahooSmall } from "react-icons/di";
import { FaAws, FaServer } from "react-icons/fa6";
import {
  MdApps,
  MdArticle,
  MdAudioFile,
  MdCode,
  MdCoPresent,
  MdDataObject,
  MdDescription,
  MdFolder,
  MdFolderZip,
  MdImage,
  MdInsertDriveFile,
  MdLink,
  MdLock,
  MdMovie,
  MdPictureAsPdf,
  MdSearch,
  MdSettings,
  MdStorage,
  MdTableChart,
  MdTerminal,
} from "react-icons/md";
import { RiOpenaiFill } from "react-icons/ri";
import {
  SiApple,
  SiBaidu,
  SiBilibili,
  SiCentos,
  SiClaude,
  SiCss,
  SiDebian,
  SiDocker,
  SiDuckduckgo,
  SiFedora,
  SiGithub,
  SiGitlab,
  SiGo,
  SiGoogle,
  SiGooglecloud,
  SiGooglegemini,
  SiHtml5,
  SiJavascript,
  SiKubernetes,
  SiLinux,
  SiMongodb,
  SiMysql,
  SiNginx,
  SiNodedotjs,
  SiPhp,
  SiPostgresql,
  SiPython,
  SiRedis,
  SiRust,
  SiTypescript,
  SiUbuntu,
  SiYoutube,
  SiZhihu,
} from "react-icons/si";
import type { FileEntry, RemoteStatsSystem } from "@/types/global";

function createLocalSvgIcon(path: string): IconType {
  const src = `${import.meta.env.BASE_URL}${path.replace(/^\//, "")}`;
  const LocalSvgIcon: IconType = ({ className, size, style, title }) => {
    const dimension = size ?? "1em";

    return (
      <img
        src={src}
        alt=""
        aria-hidden={title ? undefined : true}
        title={title}
        className={className}
        draggable={false}
        style={{
          display: "inline-block",
          width: dimension,
          height: dimension,
          objectFit: "contain",
          verticalAlign: "-0.125em",
          ...style,
        }}
      />
    );
  };

  return LocalSvgIcon;
}

function createDataUrlIcon(src: string): IconType {
  const DataUrlIcon: IconType = ({ className, size, style, title }) => {
    const dimension = size ?? "1em";

    return (
      <img
        src={src}
        alt=""
        aria-hidden={title ? undefined : true}
        title={title}
        className={className}
        draggable={false}
        style={{
          display: "inline-block",
          width: dimension,
          height: dimension,
          objectFit: "contain",
          verticalAlign: "-0.125em",
          ...style,
        }}
      />
    );
  };

  return DataUrlIcon;
}

export interface QuickIconDef {
  icon: IconType;
  color: string;
}

export const QUICK_ICONS: Record<string, QuickIconDef> = {
  docker: { icon: SiDocker, color: "#2496ed" },
  k8s: { icon: SiKubernetes, color: "#326ce5" },
  linux: { icon: SiLinux, color: "#FCC624" },
  ubuntu: { icon: SiUbuntu, color: "#E95420" },
  debian: { icon: SiDebian, color: "#A81D33" },
  centos: { icon: SiCentos, color: "#262577" },
  fedora: { icon: SiFedora, color: "#3C4FB1" },
  apple: { icon: SiApple, color: "#A2AAAD" },
  github: { icon: SiGithub, color: "#181717" },
  gitlab: { icon: SiGitlab, color: "#FC6D26" },
  nginx: { icon: SiNginx, color: "#009639" },
  redis: { icon: SiRedis, color: "#DC382D" },
  postgres: { icon: SiPostgresql, color: "#4169E1" },
  mysql: { icon: SiMysql, color: "#4479A1" },
  mongodb: { icon: SiMongodb, color: "#47A248" },
  python: { icon: SiPython, color: "#3776AB" },
  js: { icon: SiJavascript, color: "#F7DF1E" },
  ts: { icon: SiTypescript, color: "#3178C6" },
  rust: { icon: SiRust, color: "#000000" },
  go: { icon: SiGo, color: "#00ADD8" },
  node: { icon: SiNodedotjs, color: "#339933" },
  php: { icon: SiPhp, color: "#777BB4" },
  aws: { icon: FaAws, color: "#232F3E" },
  gcp: { icon: SiGooglecloud, color: "#4285F4" },
};

export type QuickIconName = keyof typeof QUICK_ICONS;

/** Mainstream OS / distro icons. */
export const SYSTEM_ICONS: Record<string, QuickIconDef> = {
  windows: { icon: createLocalSvgIcon("/icons/os/windows.svg"), color: "currentColor" },
  apple: { icon: createLocalSvgIcon("/icons/os/apple.svg"), color: "currentColor" },
  android: { icon: createLocalSvgIcon("/icons/os/android.svg"), color: "currentColor" },
  linux: { icon: createLocalSvgIcon("/icons/os/linux.svg"), color: "currentColor" },
  ubuntu: { icon: createLocalSvgIcon("/icons/os/Ubuntu.svg"), color: "currentColor" },
  debian: { icon: createLocalSvgIcon("/icons/os/Debian.svg"), color: "currentColor" },
  centos: { icon: createLocalSvgIcon("/icons/os/Centos.svg"), color: "currentColor" },
  fedora: { icon: createLocalSvgIcon("/icons/os/Fedora.svg"), color: "currentColor" },
  arch: { icon: createLocalSvgIcon("/icons/os/archlinux.svg"), color: "currentColor" },
  manjaro: { icon: createLocalSvgIcon("/icons/os/manjaro.svg"), color: "currentColor" },
  opensuse: { icon: createLocalSvgIcon("/icons/os/openSUSE.svg"), color: "currentColor" },
  rocky: { icon: createLocalSvgIcon("/icons/os/rocky-linux.svg"), color: "currentColor" },
  alma: { icon: createLocalSvgIcon("/icons/os/AlmaLinux.svg"), color: "currentColor" },
  alpine: { icon: createLocalSvgIcon("/icons/os/Alpine-Linux.svg"), color: "currentColor" },
  kali: { icon: createLocalSvgIcon("/icons/os/kalilinux.svg"), color: "currentColor" },
  mint: { icon: createLocalSvgIcon("/icons/os/linux-mint.svg"), color: "currentColor" },
  nixos: { icon: createLocalSvgIcon("/icons/os/Nixos.svg"), color: "currentColor" },
  h3c: { icon: createLocalSvgIcon("/icons/os/H3C.svg"), color: "currentColor" },
  k8s: { icon: createLocalSvgIcon("/icons/os/K8s.svg"), color: "currentColor" },
  gentoo: { icon: createLocalSvgIcon("/icons/os/Gentoo.svg"), color: "currentColor" },
  raspberrypi: { icon: createLocalSvgIcon("/icons/os/Raspberrypi.svg"), color: "currentColor" },
  openwrt: { icon: createLocalSvgIcon("/icons/os/OpenWrt.svg"), color: "currentColor" },
  "alibaba-cloud-linux": {
    icon: createLocalSvgIcon("/icons/os/AlibabaCloudLinux.svg"),
    color: "currentColor",
  },
  anolis: { icon: createLocalSvgIcon("/icons/os/AnolisOS.svg"), color: "currentColor" },
  deepin: { icon: createLocalSvgIcon("/icons/os/Deepin_A.svg"), color: "currentColor" },
  kylin: { icon: createLocalSvgIcon("/icons/os/kylin.svg"), color: "currentColor" },
  openeuler: { icon: createLocalSvgIcon("/icons/os/OpenEuler.svg"), color: "currentColor" },
  tencentos: { icon: createLocalSvgIcon("/icons/os/TencentOS.svg"), color: "currentColor" },
  uos: { icon: createLocalSvgIcon("/icons/os/uos.svg"), color: "currentColor" },
  aws: { icon: createLocalSvgIcon("/icons/os/aws.svg"), color: "currentColor" },
  huawei: { icon: createLocalSvgIcon("/icons/os/huawei.svg"), color: "currentColor" },
  git: { icon: createLocalSvgIcon("/icons/os/git.svg"), color: "currentColor" },
  cmd: { icon: createLocalSvgIcon("/icons/os/cmd.svg"), color: "currentColor" },
  powershell: { icon: createLocalSvgIcon("/icons/os/powershell.svg"), color: "currentColor" },
};

export type SystemIconName = keyof typeof SYSTEM_ICONS;

const CONNECTION_ICON_ALIASES: Record<string, string> = {
  "alibaba-cloudlinux": "alibaba-cloud-linux",
  alibabacloudlinux: "alibaba-cloud-linux",
  "alibaba-linux": "alibaba-cloud-linux",
  almalinux: "alma",
  "alma-linux": "alma",
  "alpine-linux": "alpine",
  alpinelinux: "alpine",
  anolisos: "anolis",
  "anolis-os": "anolis",
  archlinux: "arch",
  "arch-linux": "arch",
  amazon: "aws",
  "amazon-linux": "aws",
  amazonlinux: "aws",
  "aws-linux": "aws",
  "deepin-a": "deepin",
  "command-prompt": "cmd",
  commandprompt: "cmd",
  gitbash: "git",
  "git-bash": "git",
  "huawei-cloud": "huawei",
  huaweicloud: "huawei",
  "kali-linux": "kali",
  kalilinux: "kali",
  linuxmint: "mint",
  "linux-mint": "mint",
  "nix-os": "nixos",
  "open-euler": "openeuler",
  "open-suse": "opensuse",
  "open-wrt": "openwrt",
  raspberry: "raspberrypi",
  "raspberry-pi": "raspberrypi",
  "rocky-linux": "rocky",
  rockylinux: "rocky",
  tencent: "tencentos",
  "tencent-os": "tencentos",
  tencentlinux: "tencentos",
  ps: "powershell",
  pwsh: "powershell",
  "power-shell": "powershell",
};

function normalizeConnectionIconKey(iconKey: string): string {
  return iconKey
    .trim()
    .replace(/\.svg$/i, "")
    .replace(/_/g, "-")
    .toLowerCase();
}

export function isCustomConnectionIcon(iconKey?: string | null): boolean {
  const trimmed = iconKey?.trim();
  if (!trimmed) return false;

  return /^data:image\/(?:png|jpeg|jpg|webp|bmp|gif);base64,[A-Za-z0-9+/]+={0,2}$/i.test(trimmed);
}

/**
 * Default "server" glyph offered in several theme-friendly colors.
 *
 * These are the fallback icons used when a connection has no brand/system icon.
 * The keys are stored verbatim in `connection.icon`. Colors are fixed mid-tone
 * hues (Tailwind ~400 level) chosen to read well on both light and dark themes,
 * echoing the accent families used across the bundled themes in `lib/themes.ts`
 * (blue / emerald / amber / rose / violet / cyan / slate).
 */
export const SERVER_ICONS: Record<string, QuickIconDef> = {
  server: { icon: FaServer, color: "#60a5fa" }, // blue-400
  "server-emerald": { icon: FaServer, color: "#34d399" }, // emerald-400
  "server-amber": { icon: FaServer, color: "#fbbf24" }, // amber-400
  "server-rose": { icon: FaServer, color: "#fb7185" }, // rose-400
  "server-violet": { icon: FaServer, color: "#a78bfa" }, // violet-400
  "server-cyan": { icon: FaServer, color: "#22d3ee" }, // cyan-400
  "server-slate": { icon: FaServer, color: "#94a3b8" }, // slate-400
};

export type ServerIconName = keyof typeof SERVER_ICONS;

/** Linux-flavored default glyphs mirroring the server default color row. */
export const LINUX_ICONS: Record<string, QuickIconDef> = {
  "linux-default": {
    icon: SiLinux,
    color: SERVER_ICONS.server.color,
  },
  "linux-emerald": {
    icon: SiLinux,
    color: SERVER_ICONS["server-emerald"].color,
  },
  "linux-amber": {
    icon: SiLinux,
    color: SERVER_ICONS["server-amber"].color,
  },
  "linux-rose": {
    icon: SiLinux,
    color: SERVER_ICONS["server-rose"].color,
  },
  "linux-violet": {
    icon: SiLinux,
    color: SERVER_ICONS["server-violet"].color,
  },
  "linux-cyan": {
    icon: SiLinux,
    color: SERVER_ICONS["server-cyan"].color,
  },
  "linux-slate": {
    icon: SiLinux,
    color: SERVER_ICONS["server-slate"].color,
  },
};

export type LinuxIconName = keyof typeof LINUX_ICONS;

/** Canonical default icon used when a connection has no icon configured. */
export const DEFAULT_CONNECTION_ICON: ServerIconName = "server";
export const DEFAULT_CONNECTION_ICON_COLOR = SERVER_ICONS[DEFAULT_CONNECTION_ICON].color;

/** Merged lookup for all connection icons (default servers + services + systems). */
export const CONNECTION_ICONS: Record<string, QuickIconDef> = {
  ...SERVER_ICONS,
  ...LINUX_ICONS,
  ...QUICK_ICONS,
  ...SYSTEM_ICONS,
};

/**
 * Resolve a connection's stored icon key to a renderable icon + color.
 *
 * Falsy or unknown keys fall back to the canonical default server icon, so the
 * "no icon" state renders identically everywhere (session form preview, saved
 * connections list, tabs, header) instead of diverging per call site.
 */
export function resolveConnectionIcon(iconKey?: string | null): QuickIconDef {
  if (iconKey && CONNECTION_ICONS[iconKey]) {
    return CONNECTION_ICONS[iconKey];
  }

  if (iconKey) {
    if (isCustomConnectionIcon(iconKey)) {
      return { icon: createDataUrlIcon(iconKey.trim()), color: "currentColor" };
    }

    const normalizedKey = normalizeConnectionIconKey(iconKey);
    const resolvedKey = CONNECTION_ICON_ALIASES[normalizedKey] ?? normalizedKey;
    if (CONNECTION_ICONS[resolvedKey]) {
      return CONNECTION_ICONS[resolvedKey];
    }
  }

  return SERVER_ICONS[DEFAULT_CONNECTION_ICON];
}

function normalizeRemoteSystemText(system: Pick<RemoteStatsSystem, "os" | "arch">): string {
  return `${system.os ?? ""} ${system.arch ?? ""}`
    .trim()
    .replace(/[_./]/g, "-")
    .replace(/\s+/g, " ")
    .toLowerCase();
}

function hasRemoteSystemToken(text: string, token: string): boolean {
  return new RegExp(`(^|[^a-z0-9])${token}([^a-z0-9]|$)`, "i").test(text);
}

function hasRemoteSystemDistroMatch(text: string, needle: string): boolean {
  if (/^[a-z0-9]+$/i.test(needle)) {
    return hasRemoteSystemToken(text, needle);
  }

  return text.includes(needle);
}

export function inferConnectionIconKeyFromRemoteSystem(
  system: Pick<RemoteStatsSystem, "os" | "arch"> | null | undefined,
): string | null {
  if (!system) return null;

  const text = normalizeRemoteSystemText(system);
  if (!text) return null;

  const distroMatches: Array<[string[], string]> = [
    [
      ["alibaba cloud linux", "alibaba-cloud-linux", "aliyun linux", "alinux"],
      "alibaba-cloud-linux",
    ],
    [["amazon linux", "amzn", "aws linux"], "aws"],
    [["alma linux", "almalinux"], "alma"],
    [["alpine"], "alpine"],
    [["anolis"], "anolis"],
    [["arch linux", "arch-linux", "archlinux", "arch"], "arch"],
    [["centos", "cent os"], "centos"],
    [["debian"], "debian"],
    [["deepin"], "deepin"],
    [["fedora"], "fedora"],
    [["huawei", "opencloudos"], "huawei"],
    [["kali"], "kali"],
    [["kylin"], "kylin"],
    [["linux mint", "linuxmint"], "mint"],
    [["nixos", "nix os"], "nixos"],
    [["open euler", "openeuler"], "openeuler"],
    [["openwrt", "open-wrt"], "openwrt"],
    [["opensuse", "open suse", "sles", "suse"], "opensuse"],
    [["rocky"], "rocky"],
    [["tencent", "tlinux"], "tencentos"],
    [["ubuntu"], "ubuntu"],
    [["uniontech", "uos"], "uos"],
  ];

  for (const [needles, iconKey] of distroMatches) {
    if (needles.some((needle) => hasRemoteSystemDistroMatch(text, needle))) {
      return iconKey;
    }
  }

  if (text.includes("windows") || text.includes("mingw") || text.includes("msys")) {
    return "windows";
  }
  if (
    text.includes("darwin") ||
    text.includes("macos") ||
    text.includes("mac os") ||
    text.includes("os x")
  ) {
    return "apple";
  }
  if (text.includes("linux") || text.includes("gnu")) {
    return "linux";
  }

  return null;
}

export const SEARCH_ICONS: Record<string, QuickIconDef> = {
  google: { icon: SiGoogle, color: "#4285F4" },
  duckduckgo: { icon: SiDuckduckgo, color: "#DE5833" },
  baidu: { icon: SiBaidu, color: "#2932E1" },
  bilibili: { icon: SiBilibili, color: "#00A1D6" },
  zhihu: { icon: SiZhihu, color: "#0084FF" },
  youtube: { icon: SiYoutube, color: "#FF0000" },
  github: { icon: SiGithub, color: "#181717" },
  gitlab: { icon: SiGitlab, color: "#FC6D26" },
  bing: { icon: DiBingSmall, color: "#008373" },
  yahoo: { icon: DiYahooSmall, color: "#410093" },
  openai: { icon: RiOpenaiFill, color: "#10A37F" },
  claude: { icon: SiClaude, color: "#d97757" },
  gemini: { icon: SiGooglegemini, color: "#4285F4" },
  default: { icon: MdSearch, color: "currentColor" },
};

export type SearchIconName = keyof typeof SEARCH_ICONS;

export function getFileIcon(entry: FileEntry): {
  icon: ElementType;
  color: string;
} {
  if (entry.is_symlink) return { icon: MdLink, color: "#67e8f9" }; // cyan-300
  if (entry.is_dir) return { icon: MdFolder, color: "#fbbf24" }; // amber-400

  const ext = entry.name.includes(".") ? (entry.name.split(".").pop()?.toLowerCase() ?? "") : "";

  switch (ext) {
    // --- Web & Scripting ---
    case "js":
    case "jsx":
      return { icon: SiJavascript, color: "#facc15" }; // yellow-400
    case "ts":
    case "tsx":
      return { icon: SiTypescript, color: "#60a5fa" }; // blue-400
    case "html":
    case "htm":
      return { icon: SiHtml5, color: "#f97316" }; // orange-500
    case "css":
    case "scss":
    case "less":
      return { icon: SiCss, color: "#38bdf8" }; // sky-400
    case "py":
    case "pyc":
      return { icon: SiPython, color: "#3776AB" }; // python-500
    case "sh":
    case "bash":
    case "zsh":
    case "bat":
    case "ps1":
      return { icon: MdTerminal, color: "#4ade80" }; // green-400
    case "php":
      return { icon: SiPhp, color: "#777BB4" }; // php-500

    case "rs":
    case "go":
    case "c":
    case "cpp":
    case "java":
      return { icon: MdCode, color: "#f87171" }; // red-400

    // --- Data & Config ---
    case "json":
    case "yaml":
    case "yml":
    case "toml":
    case "xml":
      return { icon: MdDataObject, color: "#a78bfa" }; // violet-400
    case "ini":
    case "env":
    case "conf":
    case "config":
      return { icon: MdSettings, color: "var(--df-text-muted)" };
    case "sql":
    case "db":
    case "sqlite":
      return { icon: MdStorage, color: "#94a3b8" }; // slate-400

    // --- Text & Documents ---
    case "md":
    case "mdx":
    case "txt":
    case "rtf":
      return { icon: MdArticle, color: "var(--df-text-dimmed)" };
    case "doc":
    case "docx":
      return { icon: MdDescription, color: "#3b82f6" }; // blue-500
    case "pdf":
      return { icon: MdPictureAsPdf, color: "#ef4444" }; // red-500
    case "xls":
    case "xlsx":
    case "csv":
      return { icon: MdTableChart, color: "#16a34a" }; // green-600
    case "ppt":
    case "pptx":
      return { icon: MdCoPresent, color: "#ea580c" }; // orange-600

    // --- Media ---
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "webp":
    case "svg":
    case "ico":
      return { icon: MdImage, color: "#ec4899" }; // pink-500
    case "mp4":
    case "mkv":
    case "avi":
    case "mov":
    case "webm":
      return { icon: MdMovie, color: "#8b5cf6" }; // violet-500
    case "mp3":
    case "wav":
    case "ogg":
    case "flac":
      return { icon: MdAudioFile, color: "#f59e0b" }; // amber-500

    // --- Archives ---
    case "zip":
    case "rar":
    case "7z":
    case "tar":
    case "gz":
    case "bz2":
    case "xz":
      return { icon: MdFolderZip, color: "#f59e0b" }; // amber-500

    // --- Misc ---
    case "exe":
    case "apk":
    case "dmg":
    case "iso":
      return { icon: MdApps, color: "#14b8a6" }; // teal-500
    case "lock":
      return { icon: MdLock, color: "var(--df-text-muted)" };

    default:
      if (entry.name.startsWith(".")) {
        return { icon: MdSettings, color: "var(--df-text-muted)" };
      }
      return { icon: MdInsertDriveFile, color: "var(--df-text-muted)" };
  }
}
