import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  assetName,
  stageInstaller,
  targets,
  validateVersions,
  verifyAssets,
} from "./release.mjs";

test("validates matching versions and release tag", () => {
  assert.equal(validateVersions(Array(5).fill("0.4.1"), "v0.4.1"), "0.4.1");
  assert.equal(
    validateVersions(Array(5).fill("0.4.2-beta.1"), "v0.4.2-beta.1"),
    "0.4.2-beta.1",
  );
  assert.equal(validateVersions(Array(5).fill("0.4.1")), "0.4.1");
});

test("rejects mismatched versions, tags, and unsafe version text", () => {
  assert.throws(() => validateVersions(["0.4.1", "0.4.2"]), /do not match/);
  assert.throws(() => validateVersions(["0.4.1", undefined]), /do not match/);
  assert.throws(() => validateVersions(["0.4.1"], "v0.4.2"), /Release tag/);
  assert.throws(() => validateVersions(["../../file"]), /Invalid/);
});

test("names the supported platforms without collisions", () => {
  const names = Object.keys(targets).map((target) =>
    assetName("0.4.1", target),
  );
  assert.equal(new Set(names).size, 3);
  assert.ok(names[0].endsWith("macos_aarch64.dmg"));
  assert.ok(names[1].endsWith("macos_x64.dmg"));
  assert.ok(names[2].endsWith("windows_x64_setup.exe"));
  assert.throws(() => assetName("0.4.1", "unknown"), /Unsupported/);
});

test("stages only installers, preserves bytes, and validates all checksums", () => {
  const root = mkdtempSync(join(tmpdir(), "owon-release-test-"));
  try {
    const project = join(root, "project");
    const assets = join(root, "assets");
    for (const [target, spec] of Object.entries(targets)) {
      const folder = join(
        project,
        "src-tauri/target",
        target,
        "release/bundle",
        spec.bundle,
      );
      mkdirSync(folder, { recursive: true });
      const bytes = Buffer.from(`synthetic installer for ${target}`);
      writeFileSync(join(folder, `installer${spec.ext}`), bytes);
      const name = stageInstaller(project, assets, "0.4.1", target);
      assert.deepEqual(readFileSync(join(assets, name)), bytes);
      assert.throws(
        () => stageInstaller(project, assets, "0.4.1", target),
        /EEXIST/,
      );
    }
    assert.equal(verifyAssets(assets, "0.4.1").length, 3);
    const windows = join(assets, assetName("0.4.1", "x86_64-pc-windows-msvc"));
    writeFileSync(windows, "tampered installer");
    assert.throws(() => verifyAssets(assets, "0.4.1"), /Checksum mismatch/);
    writeFileSync(join(assets, "unexpected.txt"), "must not be uploaded");
    assert.throws(
      () => verifyAssets(assets, "0.4.1"),
      /exactly the three installers/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
