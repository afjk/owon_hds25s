import { createHash } from "node:crypto";
import {
  constants,
  copyFileSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const targets = {
  "aarch64-apple-darwin": {
    platform: "macos_aarch64",
    bundle: "dmg",
    ext: ".dmg",
  },
  "x86_64-apple-darwin": { platform: "macos_x64", bundle: "dmg", ext: ".dmg" },
  "x86_64-pc-windows-msvc": {
    platform: "windows_x64_setup",
    bundle: "nsis",
    ext: ".exe",
  },
};

export function validateVersions(versions, tag = "") {
  const version = versions[0];
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
    throw new Error("Invalid application version");
  }
  if (versions.some((value) => value !== version)) {
    throw new Error(
      `Application versions do not match: ${versions.join(", ")}`,
    );
  }
  if (tag && tag !== `v${version}`) {
    throw new Error(`Release tag must be v${version}, received ${tag}`);
  }
  return version;
}

export function readVersion(project, tag = "") {
  const json = (file) => JSON.parse(readFileSync(join(project, file), "utf8"));
  const pkg = json("package.json");
  const lock = json("package-lock.json");
  const tauri = json("src-tauri/tauri.conf.json");
  const cargo = readFileSync(join(project, "src-tauri/Cargo.toml"), "utf8");
  const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  return validateVersions(
    [
      pkg.version,
      lock.version,
      lock.packages?.[""]?.version,
      tauri.version,
      cargoVersion,
    ],
    tag,
  );
}

export function assetName(version, target) {
  const spec = targets[target];
  if (!spec) throw new Error(`Unsupported release target: ${target}`);
  return `OWON-Scope_${version}_${spec.platform}${spec.ext}`;
}

const checksum = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");
const checksumText = (path, name) => `${checksum(path)}  ${name}\n`;

export function stageInstaller(project, assets, version, target) {
  const spec = targets[target];
  if (!spec) throw new Error(`Unsupported release target: ${target}`);
  const bundle = join(
    project,
    "src-tauri/target",
    target,
    "release/bundle",
    spec.bundle,
  );
  const candidates = readdirSync(bundle).filter((name) =>
    name.endsWith(spec.ext),
  );
  if (candidates.length !== 1) {
    throw new Error(
      `Expected exactly one ${spec.ext} installer, found ${candidates.length}`,
    );
  }
  const source = join(bundle, candidates[0]);
  if (!lstatSync(source).isFile() || lstatSync(source).size === 0) {
    throw new Error("Installer must be a nonempty regular file");
  }
  mkdirSync(assets, { recursive: true });
  const name = assetName(version, target);
  const destination = join(assets, name);
  copyFileSync(source, destination, constants.COPYFILE_EXCL);
  writeFileSync(`${destination}.sha256`, checksumText(destination, name), {
    flag: "wx",
  });
  return name;
}

export function verifyAssets(assets, version) {
  const installers = Object.keys(targets).map((target) =>
    assetName(version, target),
  );
  const expected = installers
    .flatMap((name) => [name, `${name}.sha256`])
    .sort();
  const actual = readdirSync(assets).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      "Release assets must contain exactly the three installers and their checksums",
    );
  }
  for (const name of expected) {
    if (!lstatSync(join(assets, name)).isFile()) {
      throw new Error(`Release asset is not a regular file: ${name}`);
    }
  }
  for (const name of installers) {
    const installer = join(assets, name);
    if (lstatSync(installer).size === 0)
      throw new Error(`Empty installer: ${name}`);
    if (
      readFileSync(`${installer}.sha256`, "utf8") !==
      checksumText(installer, name)
    ) {
      throw new Error(`Checksum mismatch: ${name}`);
    }
  }
  return installers;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  const project = join(root, "tauri-app");
  const assets = join(root, "release-assets");
  const version = readVersion(project, process.env.RELEASE_TAG ?? "");
  switch (process.argv[2]) {
    case "check-version":
      console.log(`Validated OWON Scope ${version}`);
      break;
    case "stage":
      console.log(
        stageInstaller(project, assets, version, process.env.BUILD_TARGET),
      );
      break;
    case "verify":
      console.log(
        `Verified release assets: ${verifyAssets(assets, version).join(", ")}`,
      );
      break;
    default:
      throw new Error("Usage: release.mjs check-version | stage | verify");
  }
}
