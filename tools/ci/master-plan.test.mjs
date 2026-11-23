// node --test tools/ci/
import test from "node:test";
import assert from "node:assert/strict";
import { REQUIRED, decide, isKeyInput, latestStates } from "./master-plan.mjs";

const ok = REQUIRED.ci.map((context, id) => ({ id, context, status: "success" }));
const base = {
  ref: "refs/heads/master",
  parents: ["a", "b"],
  tree: "T",
  headTree: "T",
  statuses: ok,
  changed: ["qml/Main.qml"],
  required: REQUIRED.ci,
};

test("a merge whose tree is the green PR head's tree skips", () => {
  assert.equal(decide(base).run, false);
});

test("tags, other branches, plain commits run everything", () => {
  assert.equal(decide({ ...base, ref: "refs/tags/v1.0.0" }).run, true);
  assert.equal(decide({ ...base, parents: ["a"] }).run, true);
});

test("a different tree runs: master moved since the PR was tested", () => {
  assert.equal(decide({ ...base, headTree: "U" }).run, true);
  assert.equal(decide({ ...base, headTree: undefined }).run, true);
});

test("a missing or failed PR status runs", () => {
  assert.equal(decide({ ...base, statuses: ok.slice(1) }).run, true);
  const failed = ok.map((s, i) => (i === 0 ? { ...s, status: "failure" } : s));
  assert.equal(decide({ ...base, statuses: failed }).run, true);
});

test("a re-run's later status replaces the earlier one", () => {
  const rerun = [...ok.map((s) => ({ ...s, id: s.id + 10, status: "failure" })), ...ok.map((s) => ({ ...s, id: s.id + 20 }))];
  assert.equal(decide({ ...base, statuses: rerun }).run, false);
  assert.equal(latestStates(rerun).size, 3);
});

test("a changed cache-key input runs, so master saves the caches", () => {
  assert.equal(decide({ ...base, changed: ["crates/envio-core/Cargo.toml"] }).run, true);
  assert.equal(decide({ ...base, changed: ["docs/a.md", "Cargo.lock"] }).run, true);
  assert.equal(isKeyInput("qt/Cargo.lock"), true);
  assert.equal(isKeyInput("qt/src/lib.rs"), false);
  assert.equal(isKeyInput(".forgejo/workflows/ci.yml"), true);
});

test("click needs its own green PR status", () => {
  const click = { ...base, required: REQUIRED.click, statuses: ok };
  assert.equal(decide(click).run, true);
  const green = [{ id: 1, context: REQUIRED.click[0], status: "success" }];
  assert.equal(decide({ ...click, statuses: green }).run, false);
});
