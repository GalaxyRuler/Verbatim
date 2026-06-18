import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";

type ReleaseAsset = {
  name: string;
  browser_download_url: string;
  size: number;
};

type GitHubRelease = {
  tag_name: string;
  html_url: string;
  published_at: string;
  assets: ReleaseAsset[];
};

type DownloadLink = {
  key: "windowsSetup" | "windowsMsi" | "macDmg" | "linuxDeb";
  platformEn: string;
  platformAr: string;
  detailEn: string;
  detailAr: string;
  label: string;
  asset: ReleaseAsset;
};

type ReleaseData = {
  version: string;
  tag: string;
  releaseUrl: string;
  publishedAt: string;
  generatedAt: string;
  downloads: DownloadLink[];
};

const OWNER = process.env.WEBSITE_RELEASE_OWNER ?? "GalaxyRuler";
const REPO = process.env.WEBSITE_RELEASE_REPO ?? "Verbatim";
const SITE_ORIGIN =
  process.env.WEBSITE_ORIGIN ?? "https://verbatim.alkulaib.io";
const OUTPUT_DIR = path.resolve(
  process.env.WEBSITE_OUTPUT_DIR ?? "website/dist",
);
const CHECK_ONLY = process.argv.includes("--check");

const expectedAssets: Array<{
  key: DownloadLink["key"];
  platformEn: string;
  platformAr: string;
  detailEn: string;
  detailAr: string;
  label: string;
  pattern: RegExp;
}> = [
  {
    key: "windowsSetup",
    platformEn: "Windows",
    platformAr: "ويندوز",
    detailEn: "x64 setup.exe",
    detailAr: "ملف التثبيت x64",
    label: "setup.exe",
    pattern: /^Verbatim_.*_x64-setup\.exe$/,
  },
  {
    key: "windowsMsi",
    platformEn: "Windows",
    platformAr: "ويندوز",
    detailEn: "x64 MSI",
    detailAr: "حزمة MSI x64",
    label: "MSI",
    pattern: /^Verbatim_.*_x64_en-US\.msi$/,
  },
  {
    key: "macDmg",
    platformEn: "macOS",
    platformAr: "ماك",
    detailEn: "Apple Silicon DMG",
    detailAr: "Apple Silicon بصيغة DMG",
    label: "DMG",
    pattern: /^Verbatim_.*_aarch64\.dmg$/,
  },
  {
    key: "linuxDeb",
    platformEn: "Ubuntu",
    platformAr: "أوبونتو",
    detailEn: "x64 DEB",
    detailAr: "حزمة DEB x64",
    label: "DEB",
    pattern: /^Verbatim_.*_amd64\.deb$/,
  },
];

async function fetchLatestRelease(): Promise<GitHubRelease> {
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };

  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }

  const response = await fetch(
    `https://api.github.com/repos/${OWNER}/${REPO}/releases/latest`,
    { headers },
  );

  if (!response.ok) {
    throw new Error(
      `GitHub release lookup failed: ${response.status} ${response.statusText}`,
    );
  }

  return (await response.json()) as GitHubRelease;
}

function releaseData(release: GitHubRelease): ReleaseData {
  const downloads = expectedAssets.map((definition) => {
    const asset = release.assets.find((candidate) =>
      definition.pattern.test(candidate.name),
    );

    if (!asset) {
      throw new Error(
        `Missing expected release asset for ${definition.key}: ${definition.pattern}`,
      );
    }

    return { ...definition, asset };
  });

  return {
    version: release.tag_name.replace(/^v/, ""),
    tag: release.tag_name,
    releaseUrl: release.html_url,
    publishedAt: release.published_at,
    generatedAt: new Date().toISOString(),
    downloads,
  };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function formatDate(value: string, locale: "en" | "ar"): string {
  return new Intl.DateTimeFormat(locale === "ar" ? "ar-SA" : "en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
  }).format(new Date(value));
}

function formatBytes(bytes: number, locale: "en" | "ar"): string {
  const formatter = new Intl.NumberFormat(locale === "ar" ? "ar-SA" : "en-US", {
    maximumFractionDigits: 1,
  });
  return `${formatter.format(bytes / 1024 / 1024)} MB`;
}

function markSvg(): string {
  return `<svg viewBox="0 0 256 96" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Verbatim" class="mark">
  <g fill="currentColor">
    <rect x="18" y="38" width="8" height="20" rx="4"></rect>
    <rect x="34" y="25" width="8" height="46" rx="4"></rect>
    <rect x="50" y="14" width="8" height="68" rx="4"></rect>
    <rect x="66" y="25" width="8" height="46" rx="4"></rect>
    <rect x="82" y="34" width="8" height="28" rx="4"></rect>
    <rect x="98" y="39" width="8" height="18" rx="4"></rect>
    <rect x="114" y="43" width="8" height="10" rx="4"></rect>
    <circle cx="136" cy="48" r="4"></circle>
    <circle cx="154" cy="48" r="4"></circle>
    <circle cx="172" cy="48" r="4"></circle>
    <circle cx="190" cy="48" r="4"></circle>
    <circle cx="208" cy="48" r="4"></circle>
  </g>
  <rect x="232" y="16" width="7" height="64" rx="3.5" fill="var(--accent)"></rect>
</svg>`;
}

function downloadRows(data: ReleaseData, locale: "en" | "ar"): string {
  return data.downloads
    .map((download) => {
      const platform =
        locale === "ar" ? download.platformAr : download.platformEn;
      const detail = locale === "ar" ? download.detailAr : download.detailEn;
      const aria =
        locale === "ar"
          ? `تنزيل Verbatim ${data.tag} لنظام ${platform}، ${detail}`
          : `Download Verbatim ${data.tag} for ${platform}, ${detail}`;

      return `<a class="download-row" href="${escapeHtml(download.asset.browser_download_url)}" aria-label="${escapeHtml(aria)}">
  <span>
    <strong>${escapeHtml(platform)}</strong>
    <small>${escapeHtml(detail)} · ${escapeHtml(download.label)} · ${formatBytes(download.asset.size, locale)}</small>
  </span>
  <span class="download-arrow" aria-hidden="true">↓</span>
</a>`;
    })
    .join("\n");
}

function structuredData(data: ReleaseData, locale: "en" | "ar"): string {
  const languagePath = locale === "ar" ? "/ar/" : "/en/";
  const payload = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: "Verbatim",
    applicationCategory: "UtilitiesApplication",
    operatingSystem: "Windows, macOS, Linux",
    softwareVersion: data.version,
    inLanguage: locale === "ar" ? "ar" : "en",
    url: `${SITE_ORIGIN}${languagePath}`,
    downloadUrl: data.releaseUrl,
    license: "https://github.com/GalaxyRuler/Verbatim/blob/main/LICENSE",
    author: {
      "@type": "Person",
      name: "Abdullah Al Kulaib",
      url: "https://alkulaib.io",
    },
    offers: {
      "@type": "Offer",
      price: "0",
      priceCurrency: "USD",
    },
  };

  return JSON.stringify(payload);
}

function renderPage(data: ReleaseData, locale: "en" | "ar"): string {
  const isArabic = locale === "ar";
  const lang = isArabic ? "ar" : "en";
  const dir = isArabic ? "rtl" : "ltr";
  const otherPath = isArabic ? "/en/" : "/ar/";
  const canonicalPath = isArabic ? "/ar/" : "/en/";
  const rootCanonical = isArabic ? `${SITE_ORIGIN}/ar/` : `${SITE_ORIGIN}/`;
  const title = isArabic
    ? "Verbatim - إملاء محلي وخاص"
    : "Verbatim - Local, private speech-to-text";
  const description = isArabic
    ? "Verbatim يحول الكلام إلى نص محليا على ويندوز وماك وأوبونتو. بدون حساب، بدون سحابة، وبتحديثات تنزيل تلقائية من أحدث إصدار."
    : "Verbatim is local speech-to-text for Windows, macOS, and Ubuntu. No account, no cloud, and download links that always track the latest release.";
  const releaseDate = formatDate(data.publishedAt, locale);
  const copy = isArabic
    ? {
        navFeatures: "المزايا",
        navPrivacy: "الخصوصية",
        navDownload: "التنزيل",
        language: "English",
        eyebrow: "إملاء محلي ومفتوح المصدر",
        h1: "حوّل كلامك إلى نص في أي تطبيق.",
        sub: "اضغط الاختصار، تحدث، ودع Verbatim يكتب النص على جهازك. أحدث إصدار متوفر الآن بروابط مباشرة لويندوز وماك وأوبونتو.",
        primaryCta: "تنزيل أحدث إصدار",
        secondaryCta: "عرض المصدر",
        releaseLabel: "أحدث إصدار منشور",
        updatedLabel: "تاريخ الإصدار",
        privacyTitle: "صوتك يبقى على جهازك.",
        privacyText:
          "النماذج تعمل محليا بعد التنزيل. لا تحتاج إلى حساب، ولا ترسل تسجيلاتك إلى خادم.",
        featuresTitle: "مصمم للاستخدام اليومي.",
        features: [
          "اختصار عام يعمل داخل التطبيقات التي تستخدمها.",
          "نماذج محلية مع دعم Whisper و Parakeet.",
          "قاموس محلي وتصحيحات وسجل قابل للمراجعة.",
          "تحديثات داخل التطبيق عند توفر إصدار جديد.",
        ],
        downloadsTitle: "روابط التنزيل المباشرة.",
        downloadsText:
          "هذه الروابط تتجدد تلقائيا مع كل إصدار جديد حتى لا تشير الصفحة إلى نسخة قديمة.",
        note: "النسخ الحالية غير موقعة على مستوى نظام التشغيل؛ قد يظهر تحذير Windows SmartScreen أو macOS Gatekeeper عند التشغيل الأول.",
        footer:
          "Verbatim مجاني ومفتوح المصدر، مبني حول الخصوصية والعمل المحلي.",
      }
    : {
        navFeatures: "Features",
        navPrivacy: "Privacy",
        navDownload: "Download",
        language: "العربية",
        eyebrow: "Local, open source dictation",
        h1: "Turn speech into text in any app.",
        sub: "Press the shortcut, speak, and Verbatim types locally on your machine. The latest release is always linked directly for Windows, macOS, and Ubuntu.",
        primaryCta: "Download latest release",
        secondaryCta: "View source",
        releaseLabel: "Latest published release",
        updatedLabel: "Published",
        privacyTitle: "Your voice stays on your device.",
        privacyText:
          "Models run locally after download. No account is required, and your recordings are not sent to a server.",
        featuresTitle: "Built for daily use.",
        features: [
          "A global shortcut works inside the apps you already use.",
          "Local models with Whisper and Parakeet support.",
          "A local dictionary, corrections, and reviewable history.",
          "In-app updates when a new release is available.",
        ],
        downloadsTitle: "Direct download links.",
        downloadsText:
          "These links are regenerated after every release so the public site does not drift behind GitHub.",
        note: "Current public builds are unsigned at the OS level; Windows SmartScreen or macOS Gatekeeper may warn on first launch.",
        footer:
          "Verbatim is free and open source, built around privacy and local-first transcription.",
      };

  return `<!doctype html>
<html lang="${lang}" dir="${dir}">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${escapeHtml(title)}</title>
    <meta name="description" content="${escapeHtml(description)}" />
    <meta name="author" content="Abdullah Al Kulaib" />
    <meta name="robots" content="index, follow" />
    <meta name="theme-color" content="#0b0d11" />
    <link rel="canonical" href="${rootCanonical}" />
    <link rel="alternate" hreflang="en" href="${SITE_ORIGIN}/en/" />
    <link rel="alternate" hreflang="ar" href="${SITE_ORIGIN}/ar/" />
    <link rel="alternate" hreflang="x-default" href="${SITE_ORIGIN}/" />
    <meta property="og:type" content="website" />
    <meta property="og:url" content="${rootCanonical}" />
    <meta property="og:title" content="${escapeHtml(title)}" />
    <meta property="og:description" content="${escapeHtml(description)}" />
    <meta property="og:site_name" content="Verbatim" />
    <meta property="og:locale" content="${isArabic ? "ar_SA" : "en_US"}" />
    <meta property="og:locale:alternate" content="${isArabic ? "en_US" : "ar_SA"}" />
    <meta name="twitter:card" content="summary" />
    <meta name="twitter:title" content="${escapeHtml(title)}" />
    <meta name="twitter:description" content="${escapeHtml(description)}" />
    <link rel="stylesheet" href="/assets/site.css" />
    <script type="application/ld+json">${structuredData(data, locale)}</script>
  </head>
  <body>
    <header class="site-header">
      <a href="${canonicalPath}" class="brand" aria-label="Verbatim">
        ${markSvg()}
        <span>Verbatim</span>
      </a>
      <nav class="nav">
        <a href="#features">${escapeHtml(copy.navFeatures)}</a>
        <a href="#privacy">${escapeHtml(copy.navPrivacy)}</a>
        <a href="#download">${escapeHtml(copy.navDownload)}</a>
        <a href="${otherPath}" class="language">${escapeHtml(copy.language)}</a>
      </nav>
    </header>

    <main>
      <section class="hero">
        <div class="hero-copy">
          <p class="eyebrow">${escapeHtml(copy.eyebrow)}</p>
          <h1>${escapeHtml(copy.h1)}</h1>
          <p class="sub">${escapeHtml(copy.sub)}</p>
          <div class="actions">
            <a class="button primary" href="#download">${escapeHtml(copy.primaryCta)}</a>
            <a class="button secondary" href="https://github.com/GalaxyRuler/Verbatim">${escapeHtml(copy.secondaryCta)}</a>
          </div>
        </div>
        <aside class="release-card" aria-label="${escapeHtml(copy.releaseLabel)}">
          <span>${escapeHtml(copy.releaseLabel)}</span>
          <strong>${escapeHtml(data.tag)}</strong>
          <small>${escapeHtml(copy.updatedLabel)}: ${escapeHtml(releaseDate)}</small>
        </aside>
      </section>

      <section class="band" id="privacy">
        <div>
          <p class="section-index">01</p>
          <h2>${escapeHtml(copy.privacyTitle)}</h2>
        </div>
        <p>${escapeHtml(copy.privacyText)}</p>
      </section>

      <section class="features" id="features">
        <div>
          <p class="section-index">02</p>
          <h2>${escapeHtml(copy.featuresTitle)}</h2>
        </div>
        <div class="feature-grid">
          ${copy.features
            .map(
              (feature, index) => `<article>
  <span>${String(index + 1).padStart(2, "0")}</span>
  <p>${escapeHtml(feature)}</p>
</article>`,
            )
            .join("\n")}
        </div>
      </section>

      <section class="downloads" id="download">
        <div>
          <p class="section-index">03</p>
          <h2>${escapeHtml(copy.downloadsTitle)}</h2>
          <p>${escapeHtml(copy.downloadsText)}</p>
        </div>
        <div class="download-list">
          ${downloadRows(data, locale)}
        </div>
        <p class="release-note">${escapeHtml(copy.note)}</p>
        <a class="all-releases" href="${escapeHtml(data.releaseUrl)}">${escapeHtml(data.releaseUrl)}</a>
      </section>
    </main>

    <footer>
      <span>${escapeHtml(copy.footer)}</span>
      <span>© 2026 Abdullah Al Kulaib</span>
    </footer>
  </body>
</html>
`;
}

function stylesheet(): string {
  return `:root {
  color-scheme: dark;
  --bg: #080a0d;
  --panel: #10141a;
  --ink: #f6f8fb;
  --muted: #aab3c1;
  --line: #252d38;
  --accent: #3b82f6;
  --accent-2: #14b8a6;
  --warn: #f59e0b;
}

* {
  box-sizing: border-box;
}

html {
  scroll-behavior: smooth;
}

body {
  margin: 0;
  background:
    radial-gradient(circle at 15% 0%, rgba(20, 184, 166, 0.16), transparent 32rem),
    radial-gradient(circle at 85% 12%, rgba(59, 130, 246, 0.18), transparent 30rem),
    var(--bg);
  color: var(--ink);
  font-family:
    Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI",
    sans-serif;
  line-height: 1.5;
}

a {
  color: inherit;
  text-decoration: none;
}

.site-header,
main,
footer {
  width: min(1120px, calc(100% - 40px));
  margin: 0 auto;
}

.site-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  padding: 24px 0;
}

.brand {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  font-weight: 760;
  letter-spacing: 0;
}

.mark {
  width: 104px;
  height: auto;
  color: var(--ink);
}

.nav {
  display: flex;
  align-items: center;
  gap: 18px;
  color: var(--muted);
  font-size: 14px;
}

.nav a:hover,
.all-releases:hover {
  color: var(--ink);
}

.language {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 8px 12px;
}

.hero {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 320px;
  gap: 56px;
  align-items: end;
  min-height: calc(100vh - 96px);
  padding: 64px 0 96px;
}

.eyebrow,
.section-index,
.release-card span,
.release-card small,
.download-row small,
.release-note,
footer {
  color: var(--muted);
  font-size: 13px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

h1,
h2 {
  margin: 0;
  letter-spacing: 0;
  line-height: 0.96;
}

h1 {
  max-width: 820px;
  font-size: clamp(3.5rem, 10vw, 7.5rem);
}

h2 {
  font-size: clamp(2.35rem, 5vw, 4.5rem);
}

.sub {
  max-width: 680px;
  margin: 28px 0 0;
  color: var(--muted);
  font-size: 20px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 34px;
}

.button {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 13px 18px;
  font-weight: 700;
}

.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: white;
}

.secondary {
  color: var(--ink);
}

.release-card {
  border: 1px solid var(--line);
  border-radius: 8px;
  background: color-mix(in srgb, var(--panel) 84%, transparent);
  padding: 24px;
}

.release-card strong {
  display: block;
  margin: 14px 0 6px;
  color: var(--accent-2);
  font-size: 42px;
  line-height: 1;
}

.band,
.features,
.downloads {
  border-top: 1px solid var(--line);
  padding: 88px 0;
}

.band {
  display: grid;
  grid-template-columns: minmax(0, 0.9fr) minmax(0, 1fr);
  gap: 48px;
}

.band > p,
.downloads > div > p {
  color: var(--muted);
  font-size: 19px;
  max-width: 700px;
}

.feature-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 1px;
  margin-top: 40px;
  background: var(--line);
  border: 1px solid var(--line);
}

.feature-grid article {
  min-height: 190px;
  background: var(--bg);
  padding: 22px;
}

.feature-grid span {
  color: var(--accent-2);
  font-weight: 800;
}

.feature-grid p {
  margin: 42px 0 0;
  color: var(--muted);
}

.download-list {
  margin-top: 34px;
  border-top: 1px solid var(--line);
}

.download-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  border-bottom: 1px solid var(--line);
  padding: 20px 0;
}

.download-row strong {
  display: block;
  font-size: 22px;
}

.download-row small {
  display: block;
  margin-top: 4px;
}

.download-arrow {
  color: var(--accent-2);
  font-size: 24px;
}

.release-note {
  margin-top: 22px;
  text-transform: none;
  letter-spacing: 0;
}

.all-releases {
  display: inline-block;
  margin-top: 10px;
  color: var(--accent-2);
  font-size: 14px;
}

footer {
  display: flex;
  justify-content: space-between;
  gap: 20px;
  border-top: 1px solid var(--line);
  padding: 28px 0 44px;
  text-transform: none;
  letter-spacing: 0;
}

[dir="rtl"] body {
  font-family:
    "Noto Sans Arabic", Tahoma, ui-sans-serif, system-ui, -apple-system,
    BlinkMacSystemFont, "Segoe UI", sans-serif;
}

[dir="rtl"] .eyebrow,
[dir="rtl"] .section-index,
[dir="rtl"] .release-card span,
[dir="rtl"] .release-card small,
[dir="rtl"] .download-row small,
[dir="rtl"] .release-note,
[dir="rtl"] footer {
  letter-spacing: 0;
}

@media (max-width: 820px) {
  .site-header {
    align-items: flex-start;
    flex-direction: column;
  }

  .nav {
    flex-wrap: wrap;
  }

  .hero,
  .band {
    grid-template-columns: 1fr;
  }

  .hero {
    min-height: auto;
    padding: 48px 0 72px;
  }

  .feature-grid {
    grid-template-columns: 1fr;
  }

  footer {
    flex-direction: column;
  }
}
`;
}

function redirects(): string {
  return `/index.html  /en/  302
/download  /en/#download  302
/downloads  /en/#download  302
`;
}

function robots(): string {
  return `User-agent: *
Allow: /

Sitemap: ${SITE_ORIGIN}/sitemap.xml
`;
}

function sitemap(data: ReleaseData): string {
  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>${SITE_ORIGIN}/</loc>
    <lastmod>${data.publishedAt.slice(0, 10)}</lastmod>
  </url>
  <url>
    <loc>${SITE_ORIGIN}/en/</loc>
    <lastmod>${data.publishedAt.slice(0, 10)}</lastmod>
  </url>
  <url>
    <loc>${SITE_ORIGIN}/ar/</loc>
    <lastmod>${data.publishedAt.slice(0, 10)}</lastmod>
  </url>
</urlset>
`;
}

function publicReleaseJson(data: ReleaseData): string {
  return JSON.stringify(
    {
      version: data.version,
      tag: data.tag,
      releaseUrl: data.releaseUrl,
      publishedAt: data.publishedAt,
      generatedAt: data.generatedAt,
      downloads: Object.fromEntries(
        data.downloads.map((download) => [
          download.key,
          {
            name: download.asset.name,
            url: download.asset.browser_download_url,
            size: download.asset.size,
          },
        ]),
      ),
    },
    null,
    2,
  );
}

async function writeGeneratedSite(data: ReleaseData): Promise<void> {
  await rm(OUTPUT_DIR, { recursive: true, force: true });
  await mkdir(path.join(OUTPUT_DIR, "assets"), { recursive: true });
  await mkdir(path.join(OUTPUT_DIR, "en"), { recursive: true });
  await mkdir(path.join(OUTPUT_DIR, "ar"), { recursive: true });

  await Promise.all([
    writeFile(path.join(OUTPUT_DIR, "index.html"), renderPage(data, "en")),
    writeFile(
      path.join(OUTPUT_DIR, "en", "index.html"),
      renderPage(data, "en"),
    ),
    writeFile(
      path.join(OUTPUT_DIR, "ar", "index.html"),
      renderPage(data, "ar"),
    ),
    writeFile(path.join(OUTPUT_DIR, "assets", "site.css"), stylesheet()),
    writeFile(path.join(OUTPUT_DIR, "release.json"), publicReleaseJson(data)),
    writeFile(path.join(OUTPUT_DIR, "robots.txt"), robots()),
    writeFile(path.join(OUTPUT_DIR, "sitemap.xml"), sitemap(data)),
    writeFile(path.join(OUTPUT_DIR, "_redirects"), redirects()),
  ]);
}

async function assertGeneratedSite(data: ReleaseData): Promise<void> {
  const files = [
    path.join(OUTPUT_DIR, "index.html"),
    path.join(OUTPUT_DIR, "en", "index.html"),
    path.join(OUTPUT_DIR, "ar", "index.html"),
    path.join(OUTPUT_DIR, "release.json"),
  ];

  for (const file of files) {
    const content = await readFile(file, "utf8");
    if (!content.includes(data.tag)) {
      throw new Error(`${file} does not contain ${data.tag}`);
    }
  }

  const arabic = await readFile(
    path.join(OUTPUT_DIR, "ar", "index.html"),
    "utf8",
  );
  if (!arabic.includes('dir="rtl"')) {
    throw new Error('Arabic page is missing dir="rtl"');
  }

  const english = await readFile(
    path.join(OUTPUT_DIR, "en", "index.html"),
    "utf8",
  );
  if (!english.includes('dir="ltr"')) {
    throw new Error('English page is missing dir="ltr"');
  }
}

const latest = await fetchLatestRelease();
const data = releaseData(latest);

await writeGeneratedSite(data);
await assertGeneratedSite(data);

if (CHECK_ONLY) {
  console.log(`Website latest release block is valid for ${data.tag}.`);
} else {
  console.log(`Generated website for ${data.tag} in ${OUTPUT_DIR}.`);
}
