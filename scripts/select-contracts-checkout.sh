#!/usr/bin/env bash
set -euo pipefail

runtime_manifest="${RUNTIME_MANIFEST:-Cargo.toml}"
contracts_checkout="${CONTRACTS_CHECKOUT:-}"
contracts_ref="${CONTRACTS_REF:-${CONTRACTS_BRANCH:-}}"

ci_error() {
  echo "::error::$*" >&2
}

read_required_version() {
  python3 - "$runtime_manifest" <<'PY'
import re
import sys
from pathlib import Path

manifest = Path(sys.argv[1])
section = ""
found_dependency = False

for line in manifest.read_text(encoding="utf-8").splitlines():
    stripped = line.strip()
    header = re.match(r"^\[([^\]]+)\]\s*(?:#.*)?$", stripped)
    if header:
        section = header.group(1).strip()
        continue

    if section == "dependencies" and re.match(r"^\s*skenion-contracts\s*=", line):
        found_dependency = True
        dependency = line.split("=", 1)[1].strip()
        match = re.match(r'^"([^"]+)"', dependency)
        if not match:
            match = re.search(r'\bversion\s*=\s*"([^"]+)"', dependency)
        if not match:
            raise SystemExit("skenion-contracts dependency must declare a version")
        print(match.group(1))
        raise SystemExit(0)

    if section in {"dependencies.skenion-contracts", 'dependencies."skenion-contracts"'}:
        found_dependency = True
        if re.match(r"^\s*version\s*=", line):
            match = re.search(r'"([^"]+)"', line)
            if not match:
                raise SystemExit("skenion-contracts dependency version line is malformed")
            print(match.group(1))
            raise SystemExit(0)

if found_dependency:
    raise SystemExit("skenion-contracts dependency must declare a version")

raise SystemExit("skenion-contracts dependency was not found")
PY
}

read_contracts_version() {
  local manifest="$1"
  python3 - "$manifest" <<'PY'
import re
import sys
from pathlib import Path

manifest = Path(sys.argv[1])
section = ""
for line in manifest.read_text(encoding="utf-8").splitlines():
    stripped = line.strip()
    if stripped.startswith("[") and stripped.endswith("]"):
        section = stripped
        continue
    if section == "[package]" and re.match(r"^\s*version\s*=", line):
        match = re.search(r'"([^"]+)"', line)
        if not match:
            raise SystemExit("Contracts package version line is malformed")
        print(match.group(1))
        raise SystemExit(0)

raise SystemExit("Contracts package version was not found")
PY
}

required_version="$(read_required_version)"
echo "Runtime Cargo manifest requires skenion-contracts ${required_version}."

if [[ -z "${contracts_checkout}" ]]; then
  ci_error "select-contracts-checkout.sh is only for explicit Contracts integration/evidence checks."
  ci_error "Set CONTRACTS_CHECKOUT to a skenion-contracts git checkout; normal Runtime builds use the crates.io dependency."
  exit 1
fi

if [[ -z "${contracts_ref}" ]]; then
  ci_error "Set CONTRACTS_REF or CONTRACTS_BRANCH to the exact Contracts branch, tag, or local ref to validate."
  ci_error "Refusing to fall back to main."
  exit 1
fi

if [[ ! -d "${contracts_checkout}/.git" ]]; then
  ci_error "Contracts checkout '${contracts_checkout}' is not a git repository."
  exit 1
fi

cd "${contracts_checkout}"

selected_ref=""
if git ls-remote --exit-code origin "refs/heads/${contracts_ref}" >/dev/null 2>&1; then
  git fetch --depth=1 origin "+refs/heads/${contracts_ref}:refs/remotes/origin/${contracts_ref}"
  git switch --detach "refs/remotes/origin/${contracts_ref}"
  selected_ref="branch ${contracts_ref}"
elif git ls-remote --exit-code origin "refs/tags/${contracts_ref}" >/dev/null 2>&1; then
  git fetch --depth=1 origin "+refs/tags/${contracts_ref}:refs/tags/${contracts_ref}"
  git switch --detach "${contracts_ref}"
  selected_ref="tag ${contracts_ref}"
elif git cat-file -e "${contracts_ref}^{commit}" 2>/dev/null; then
  git switch --detach "${contracts_ref}"
  selected_ref="local ref ${contracts_ref}"
else
  ci_error "No Contracts branch, tag, or local ref '${contracts_ref}' exists."
  ci_error "Runtime requires skenion-contracts ${required_version}; refusing to fall back to main."
  exit 1
fi

actual_version="$(read_contracts_version packages/rust/Cargo.toml)"
if [[ "${actual_version}" != "${required_version}" ]]; then
  ci_error "Selected Contracts ${selected_ref} has version ${actual_version}, but Runtime requires ${required_version}."
  exit 1
fi

echo "Selected Contracts ${selected_ref} for skenion-contracts ${required_version}."
