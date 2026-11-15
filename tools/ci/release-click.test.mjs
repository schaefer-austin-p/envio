// node --test tools/ci/release-click.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { changelogSection, parseTag, releaseFor } from "./release-click.mjs";

const changelog = [
  "# Changelog",
  "",
  "Intro.",
  "",
  "## [0.2.0] - 2026-11-15",
  "",
  "Release milestone:",
  "https://example.org/milestone/15",
  "",
  "### Added",
  "",
  "- Invites.",
  "",
  "## [0.1.0] - 2026-11-13",
  "",
  "### Added",
  "",
  "- Accounts.",
  "",
].join("\n");

test("a tag gives its version; a suffix marks a pre-release", () => {
  assert.deepEqual(parseTag("v0.2.0"), { version: "0.2.0", base: "0.2.0", prerelease: false });
  assert.deepEqual(parseTag("v0.3.0-rc.1"), { version: "0.3.0-rc.1", base: "0.3.0", prerelease: true });
  for (const bad of ["0.2.0", "v0.2", "v0.2.0/x", "master", "", undefined]) {
    assert.throws(() => parseTag(bad), /not a release tag/);
  }
});

test("the section stops at the next version and moves the milestone link last", () => {
  assert.equal(changelogSection(changelog, "0.2.0"), "### Added\n\n- Invites.\n\nFull list: https://example.org/milestone/15");
  assert.equal(changelogSection(changelog, "0.1.0"), "### Added\n\n- Accounts.");
  assert.equal(changelogSection(changelog, "0.3.0"), null);
});

test("a full release takes its own section and fails without one", () => {
  const r = releaseFor("v0.2.0", changelog);
  assert.equal(r.name, "Envio 0.2.0");
  assert.equal(r.prerelease, false);
  assert.match(r.body, /^### Added\n\n- Invites\./);
  assert.throws(() => releaseFor("v0.3.0", changelog), /no "## \[0\.3\.0\]" section/);
});

test("a pre-release uses the base version's notes, or a placeholder", () => {
  assert.match(releaseFor("v0.2.0-rc.1", changelog).body, /- Invites\./);
  const r = releaseFor("v0.3.0-rc.1", changelog);
  assert.equal(r.prerelease, true);
  assert.equal(r.name, "Envio 0.3.0-rc.1");
  assert.match(r.body, /^Pre-release build of v0\.3\.0-rc\.1/);
});
