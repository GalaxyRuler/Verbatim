import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

type ReleaseAsset = {
  name: string;
  browser_download_url: string;
};

type GitHubRelease = {
  tag_name: string;
  html_url: string;
  published_at: string;
  assets: ReleaseAsset[];
};

const OWNER = process.env.README_RELEASE_OWNER ?? "GalaxyRuler";
const REPO = process.env.README_RELEASE_REPO ?? "Verbatim";
const README_PATH = process.env.README_RELEASE_FILE ?? "README.md";
const START_MARKER = "<!-- latest-release:start -->";
const END_MARKER = "<!-- latest-release:end -->";
const CHECK_ONLY = process.argv.includes("--check");

const assetGroups: Array<{
  platform: string;
  patterns: Array<{ label: string; pattern: RegExp }>;
}> = [
  {
    platform: "Windows x64",
    patterns: [
      { label: "setup.exe", pattern: /^Verbatim_.*_x64-setup\.exe$/ },
      { label: "MSI", pattern: /^Verbatim_.*_x64_en-US\.msi$/ },
    ],
  },
  {
    platform: "macOS Apple Silicon",
    patterns: [{ label: "DMG", pattern: /^Verbatim_.*_aarch64\.dmg$/ }],
  },
  {
    platform: "Ubuntu x64",
    patterns: [{ label: "DEB", pattern: /^Verbatim_.*_amd64\.deb$/ }],
  },
  {
    platform: "Android",
    patterns: [
      { label: "APK", pattern: /^Verbatim_.*_android_universal\.apk$/ },
      { label: "AAB", pattern: /^Verbatim_.*_android_universal\.aab$/ },
    ],
  },
];

function assetLink(asset: ReleaseAsset, label: string): string {
  return `[${label}](${asset.browser_download_url})`;
}

function findAsset(
  assets: ReleaseAsset[],
  pattern: RegExp,
): ReleaseAsset | undefined {
  return assets.find((asset) => pattern.test(asset.name));
}

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
    {
      headers,
    },
  );

  if (!response.ok) {
    throw new Error(
      `GitHub release lookup failed: ${response.status} ${response.statusText}`,
    );
  }

  return (await response.json()) as GitHubRelease;
}

function buildReleaseBlock(release: GitHubRelease): string {
  const publishedDate = new Date(release.published_at)
    .toISOString()
    .slice(0, 10);
  const rows = assetGroups
    .map<[string, string] | undefined>(({ platform, patterns }) => {
      const links = patterns
        .map(({ label, pattern }) => {
          const asset = findAsset(release.assets, pattern);
          return asset ? assetLink(asset, label) : undefined;
        })
        .filter(Boolean)
        .join(" · ");

      return links ? [platform, links] : undefined;
    })
    .filter((row): row is [string, string] => Boolean(row));

  return [
    START_MARKER,
    "",
    `**Latest published release:** [${release.tag_name}](${release.html_url}) (${publishedDate})`,
    "",
    formatMarkdownTable(["Platform", "Direct downloads"], rows),
    "",
    END_MARKER,
  ].join("\n");
}

function formatMarkdownTable(
  headers: [string, string],
  rows: Array<[string, string]>,
): string {
  const widthA = Math.max(
    headers[0].length,
    ...rows.map(([cell]) => cell.length),
  );
  const widthB = Math.max(
    headers[1].length,
    ...rows.map(([, cell]) => cell.length),
  );
  const formatRow = ([cellA, cellB]: [string, string]) =>
    `| ${cellA.padEnd(widthA)} | ${cellB.padEnd(widthB)} |`;

  return [
    formatRow(headers),
    `| ${"-".repeat(widthA)} | ${"-".repeat(widthB)} |`,
    ...rows.map(formatRow),
  ].join("\n");
}

function replaceOrInsertBlock(readme: string, block: string): string {
  const start = readme.indexOf(START_MARKER);
  const end = readme.indexOf(END_MARKER);

  if (start !== -1 && end !== -1 && end > start) {
    return `${readme.slice(0, start)}${block}${readme.slice(end + END_MARKER.length)}`;
  }

  const anchor =
    "Download the latest Windows, macOS, Linux, and Android builds from the [Verbatim Releases page](https://github.com/GalaxyRuler/Verbatim/releases/latest).";
  const anchorIndex = readme.indexOf(anchor);

  if (anchorIndex === -1) {
    throw new Error("Could not find the Download Verbatim anchor in README.md");
  }

  const insertAt = anchorIndex + anchor.length;
  return `${readme.slice(0, insertAt)}\n\n${block}${readme.slice(insertAt)}`;
}

const readmePath = path.resolve(README_PATH);
const release = await fetchLatestRelease();
const readme = await readFile(readmePath, "utf8");
const nextReadme = replaceOrInsertBlock(readme, buildReleaseBlock(release));

if (CHECK_ONLY) {
  if (readme !== nextReadme) {
    throw new Error(
      "README latest release block is out of date. Run bun scripts/update-readme-release.ts.",
    );
  }

  console.log("README latest release block is up to date.");
} else {
  await writeFile(readmePath, nextReadme);
  console.log(`Updated README latest release block to ${release.tag_name}.`);
}
