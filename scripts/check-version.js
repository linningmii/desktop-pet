const fs = require('fs');

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function packageSection(toml, file) {
  const lines = toml.split(/\r?\n/);
  const section = [];
  let inPackage = false;

  for (const line of lines) {
    if (/^\s*\[/.test(line)) {
      if (/^\s*\[package\]\s*$/.test(line)) {
        inPackage = true;
        continue;
      }
      if (inPackage) break;
    }

    if (inPackage) section.push(line);
  }

  if (!inPackage) {
    throw new Error(`${file} is missing a [package] section.`);
  }
  return section.join('\n');
}

function tomlValue(section, key, file) {
  const match = section.match(new RegExp(`^\\s*${key}\\s*=\\s*"([^"]+)"`, 'm'));
  if (!match) {
    throw new Error(`${file} is missing package.${key}.`);
  }
  return match[1];
}

const packageJson = readJson('package.json');
const expectedVersion = process.argv[2] || packageJson.version;
const checks = [['package.json version', packageJson.version]];

const packageLock = readJson('package-lock.json');
checks.push(['package-lock.json version', packageLock.version]);
checks.push(['package-lock.json packages[""].version', packageLock.packages?.['']?.version]);

const tauriConfig = readJson('src-tauri/tauri.conf.json');
checks.push(['src-tauri/tauri.conf.json version', tauriConfig.version]);

const cargoTomlFile = 'src-tauri/Cargo.toml';
const cargoPackage = packageSection(fs.readFileSync(cargoTomlFile, 'utf8'), cargoTomlFile);
const cargoName = tomlValue(cargoPackage, 'name', cargoTomlFile);
const cargoVersion = tomlValue(cargoPackage, 'version', cargoTomlFile);
checks.push(['src-tauri/Cargo.toml package.version', cargoVersion]);

const cargoLockFile = 'src-tauri/Cargo.lock';
const cargoLock = fs.readFileSync(cargoLockFile, 'utf8');
const packageBlocks = cargoLock.split(/\n(?=\[\[package\]\]\n)/);
const lockPackage = packageBlocks.find((block) => {
  const name = block.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
  return name === cargoName;
});
if (!lockPackage) {
  throw new Error(`${cargoLockFile} is missing package "${cargoName}".`);
}
const lockVersion = lockPackage.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
checks.push([`src-tauri/Cargo.lock ${cargoName} version`, lockVersion]);

const failures = checks.filter(([, version]) => version !== expectedVersion);
if (failures.length > 0) {
  console.error(`Expected project version ${expectedVersion}, but found mismatches:`);
  for (const [label, version] of checks) {
    const marker = version === expectedVersion ? 'OK' : 'MISMATCH';
    console.error(`- ${marker}: ${label} = ${version ?? '<missing>'}`);
  }
  process.exit(1);
}

console.log(`Project version is consistent: ${expectedVersion}`);
for (const [label, version] of checks) {
  console.log(`- ${label}: ${version}`);
}
