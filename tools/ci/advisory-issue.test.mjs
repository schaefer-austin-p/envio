// node --test tools/ci/
import test from "node:test";
import assert from "node:assert/strict";
import { collectRows, ids, parseArgs, renderBody } from "./advisory-issue.mjs";

const vuln = (id, name, version, patched) => ({
  advisory: { id },
  package: { name, version },
  versions: { patched },
});
const report = (...list) => ({ vulnerabilities: { list } });

test("rows are sorted, deduplicated and labelled by lock file", () => {
  const rows = collectRows([
    { lock: "qt/Cargo.lock", report: report(vuln("B-2", "b", "1.0.0", [">=1.1.0"])) },
    { lock: "Cargo.lock", report: report(vuln("A-1", "a", "0.1.0", []), vuln("A-1", "a", "0.1.0", [])) },
  ]);
  assert.deepEqual(rows, [
    ["A-1", "a", "0.1.0", "none yet", "Cargo.lock"],
    ["B-2", "b", "1.0.0", ">=1.1.0", "qt/Cargo.lock"],
  ]);
});

test("ids are unique and sorted; the body carries them as a marker", () => {
  const rows = collectRows([
    { lock: "Cargo.lock", report: report(vuln("B-2", "b", "1", ["2"]), vuln("A-1", "a", "1", ["2"])) },
    { lock: "qt/Cargo.lock", report: report(vuln("A-1", "a", "1", ["2"])) },
  ]);
  assert.deepEqual(ids(rows), ["A-1", "B-2"]);
  assert.match(renderBody(rows), /<!-- ids: A-1,B-2 -->/);
  assert.equal(renderBody(rows), renderBody(rows));
});

test("a report of the wrong shape throws", () => {
  assert.throws(() => collectRows([{ lock: "Cargo.lock", report: {} }]), /no vulnerabilities list/);
  assert.throws(() => collectRows([{ lock: "Cargo.lock", report: null }]));
});

test("arguments name the lock file explicitly", () => {
  assert.deepEqual(parseArgs(["root=a.json", "qt=b.json"]), [
    { lock: "Cargo.lock", file: "a.json" },
    { lock: "qt/Cargo.lock", file: "b.json" },
  ]);
  assert.throws(() => parseArgs(["a.json"]));
  assert.throws(() => parseArgs([]));
});
