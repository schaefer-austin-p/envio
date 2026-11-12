// Keeps one open issue, "Security advisories in locked dependencies", in step
// with `cargo audit --json` reports (one per Cargo.lock).
//
//   node tools/ci/advisory-issue.mjs audit-root.json audit-qt.json
//
// Env: GITHUB_TOKEN, GITHUB_SERVER_URL, GITHUB_REPOSITORY (set by the runner).
// With advisories: create the issue or refresh its body (a sorted table, so a
// rerun changes nothing) and comment only when the set of IDs changed. With
// none and the issue open: comment and close it. Exit 0 unless an API call
// fails. Only vulnerabilities count; "unmaintained"/"unsound" warnings have no
// patched release to move to.
import { readFileSync } from "node:fs";

const TITLE = "Security advisories in locked dependencies";
const LABEL = "security";
const MARKER = /<!-- ids: ([^>]*?) -->/;

/** Rows [id, crate, version, fixed, lockfile] from the cargo audit reports. */
export function collectRows(reports) {
  const seen = new Set();
  const rows = [];
  for (const { lock, report } of reports) {
    for (const v of report?.vulnerabilities?.list ?? []) {
      const fixed = (v.versions?.patched ?? []).join(", ") || "none yet";
      const row = [v.advisory.id, v.package.name, v.package.version, fixed, lock];
      const key = row.join("|");
      if (!seen.has(key)) {
        seen.add(key);
        rows.push(row);
      }
    }
  }
  return rows.sort((a, b) => a.join("|").localeCompare(b.join("|")));
}

export function ids(rows) {
  return [...new Set(rows.map((r) => r[0]))].sort();
}

export function renderBody(rows) {
  const table = rows.map((r) => `| ${r[0]} | ${r[1]} | ${r[2]} | ${r[3]} | ${r[4]} |`);
  return [
    "`cargo audit` reports known vulnerabilities in the locked dependencies.",
    "This issue is kept up to date by the advisories workflow and closes itself",
    "when the lock files are clean.",
    "",
    "| Advisory | Crate | Locked | Fixed in | Lock file |",
    "| --- | --- | --- | --- | --- |",
    ...table,
    "",
    `<!-- ids: ${ids(rows).join(",")} -->`,
    "",
  ].join("\n");
}

async function main() {
  const server = process.env.GITHUB_SERVER_URL;
  const repo = process.env.GITHUB_REPOSITORY;
  const token = process.env.GITHUB_TOKEN;
  if (!server || !repo || !token) {
    throw new Error("GITHUB_SERVER_URL, GITHUB_REPOSITORY and GITHUB_TOKEN are required");
  }
  const api = `${server}/api/v1/repos/${repo}`;
  const call = async (method, path, body) => {
    const res = await fetch(api + path, {
      method,
      headers: { authorization: `token ${token}`, "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${text.slice(0, 300)}`);
    return text ? JSON.parse(text) : null;
  };

  const reports = process.argv.slice(2).map((file) => ({
    lock: file.includes("qt") ? "qt/Cargo.lock" : "Cargo.lock",
    report: JSON.parse(readFileSync(file, "utf8")),
  }));
  const rows = collectRows(reports);

  const found = await call("GET", `/issues?state=open&type=issues&labels=${LABEL}&q=${encodeURIComponent(TITLE)}`);
  const issue = found.find((i) => i.title === TITLE);

  if (rows.length === 0) {
    if (issue) {
      await call("POST", `/issues/${issue.number}/comments`, {
        body: "`cargo audit` is clean on both lock files now. Closing.",
      });
      await call("PATCH", `/issues/${issue.number}`, { state: "closed" });
      console.log(`closed #${issue.number}`);
    } else {
      console.log("no advisories, no open issue");
    }
    return;
  }

  const body = renderBody(rows);
  if (!issue) {
    const labels = await call("GET", "/labels?limit=100");
    let label = labels.find((l) => l.name === LABEL);
    if (!label) label = await call("POST", "/labels", { name: LABEL, color: "#d73a4a" });
    const made = await call("POST", "/issues", { title: TITLE, body, labels: [label.id] });
    console.log(`opened #${made.number}`);
    return;
  }

  const before = (MARKER.exec(issue.body ?? "")?.[1] ?? "").split(",").filter(Boolean).sort();
  if ((issue.body ?? "") !== body) {
    await call("PATCH", `/issues/${issue.number}`, { body });
    console.log(`updated #${issue.number}`);
  }
  if (before.join(",") !== ids(rows).join(",")) {
    await call("POST", `/issues/${issue.number}/comments`, {
      body: `The set of advisories changed: ${ids(rows).join(", ")}.`,
    });
    console.log(`commented on #${issue.number}`);
  } else {
    console.log(`#${issue.number} unchanged`);
  }
}

if (process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
