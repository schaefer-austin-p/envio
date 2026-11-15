// Puts the click package of a `v*` tag build on that tag's Forgejo release.
//
//   node tools/ci/release-click.mjs build-click/envio.aps_0.2.0_arm64.click
//   node tools/ci/release-click.mjs --check
//
// --check only finds the notes (no network); the workflow runs it before the
// build so a tag without notes fails fast.
//
// Env: GITHUB_TOKEN, GITHUB_SERVER_URL, GITHUB_REPOSITORY, GITHUB_REF_NAME
// (set by the runner; the ref name is the tag).
//
// No release for the tag yet: create one, titled "Envio <version>", with the
// version's CHANGELOG section as its notes. A pre-release tag (v0.3.0-rc.1)
// makes a pre-release; its notes are the base version's section if there is
// one. A full release tag with no CHANGELOG section fails the run, so a
// release never goes out without notes. A release that already exists keeps
// its notes (they may have been edited by hand).
//
// Each click is uploaded unless the release already has a file of that name:
// a rerun never replaces a published package.
import { readFileSync } from "node:fs";
import { basename } from "node:path";

const TAG = /^v(\d+\.\d+\.\d+)(-[0-9A-Za-z.-]+)?$/;

/** "v0.2.0" -> { version: "0.2.0", base: "0.2.0", prerelease: false }. */
export function parseTag(tag) {
  const m = TAG.exec(tag ?? "");
  if (!m) throw new Error(`"${tag}" is not a release tag (want v<major>.<minor>.<patch>[-pre])`);
  return { version: m[1] + (m[2] ?? ""), base: m[1], prerelease: Boolean(m[2]) };
}

/**
 * The body of `## [version]` in a Keep a Changelog file, without its heading
 * and with its "Release milestone:" link moved to the end. Null when the
 * version has no section.
 */
export function changelogSection(text, version) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((l) => l.startsWith(`## [${version}]`));
  if (start < 0) return null;
  let end = lines.findIndex((l, i) => i > start && l.startsWith("## ["));
  if (end < 0) end = lines.length;
  const body = lines.slice(start + 1, end);
  let milestone = null;
  const at = body.findIndex((l) => l.trim() === "Release milestone:");
  if (at >= 0 && /^https?:\/\//.test(body[at + 1]?.trim() ?? "")) {
    milestone = body[at + 1].trim();
    body.splice(at, 2);
  }
  const notes = body.join("\n").trim();
  return milestone ? `${notes}\n\nFull list: ${milestone}` : notes;
}

/** What to create for a tag: { name, body, prerelease }. Throws without notes. */
export function releaseFor(tag, changelog) {
  const { version, base, prerelease } = parseTag(tag);
  const section = changelogSection(changelog, version) ?? (prerelease ? changelogSection(changelog, base) : null);
  if (section === null && !prerelease) {
    throw new Error(`CHANGELOG.md has no "## [${version}]" section; add it before tagging`);
  }
  const body = section ?? `Pre-release build of ${tag}. Not for daily use.`;
  return { name: `Envio ${version}`, body, prerelease };
}

async function main() {
  if (process.argv[2] === "--check") {
    const { name, prerelease } = releaseFor(process.env.GITHUB_REF_NAME, readFileSync("CHANGELOG.md", "utf8"));
    console.log(`release notes found for ${name}${prerelease ? " (pre-release)" : ""}`);
    return;
  }
  const { GITHUB_SERVER_URL: server, GITHUB_REPOSITORY: repo, GITHUB_TOKEN: token } = process.env;
  const tag = process.env.GITHUB_REF_NAME;
  if (!server || !repo || !token || !tag) {
    throw new Error("GITHUB_SERVER_URL, GITHUB_REPOSITORY, GITHUB_TOKEN and GITHUB_REF_NAME are required");
  }
  const files = process.argv.slice(2);
  if (files.length === 0) throw new Error("usage: release-click.mjs <file.click>...");

  const api = `${server}/api/v1/repos/${repo}`;
  const call = async (method, path, body) => {
    const json = body !== undefined && !(body instanceof FormData);
    const res = await fetch(api + path, {
      method,
      headers: { authorization: `token ${token}`, ...(json ? { "content-type": "application/json" } : {}) },
      body: json ? JSON.stringify(body) : body,
    });
    const text = await res.text();
    if (method === "GET" && res.status === 404) return null;
    if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${text.slice(0, 300)}`);
    return text ? JSON.parse(text) : null;
  };

  let release = await call("GET", `/releases/tags/${encodeURIComponent(tag)}`);
  if (release) {
    console.log(`release ${release.name} exists; its notes are left as they are`);
  } else {
    const { name, body, prerelease } = releaseFor(tag, readFileSync("CHANGELOG.md", "utf8"));
    release = await call("POST", "/releases", { tag_name: tag, name, body, draft: false, prerelease });
    console.log(`created release ${name}${prerelease ? " (pre-release)" : ""}`);
  }

  const have = new Set((release.assets ?? []).map((a) => a.name));
  for (const file of files) {
    const name = basename(file);
    if (have.has(name)) {
      console.log(`${name} is already on the release; not replaced`);
      continue;
    }
    const form = new FormData();
    form.append("attachment", new Blob([readFileSync(file)]), name);
    const asset = await call("POST", `/releases/${release.id}/assets?name=${encodeURIComponent(name)}`, form);
    console.log(`uploaded ${name} (${asset.size} bytes): ${asset.browser_download_url}`);
  }
}

if (process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
