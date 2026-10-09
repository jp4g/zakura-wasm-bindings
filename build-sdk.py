#!/usr/bin/env python3
"""Build the SDK's native wallet and codecs with locked, verified inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent
TOOLS = {
    'wasi-sdk-27.0-x86_64-linux': (
        'https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-27/wasi-sdk-27.0-x86_64-linux.tar.gz',
        'b7d4d944c88503e4f21d84af07ac293e3440b1b6210bfd7fe78e0afd92c23bc2'),
    'wasm-bindgen-0.2.128-x86_64-unknown-linux-musl': (
        'https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.128/wasm-bindgen-0.2.128-x86_64-unknown-linux-musl.tar.gz',
        'b51f0208fdff83515a787bd8ab9ac5865ed84dabb66d0c709957bb59793c645f'),
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob('*')) if p.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'build/sdk', help='new output directory (default: build/sdk)')
    parser.add_argument('--cache', type=Path, default=ROOT / '.cache/sdk', help='reusable downloads and Cargo cache')
    args = parser.parse_args()
    if sys.version_info < (3, 12):
        raise RuntimeError('Python 3.12 or newer is required')
    if (platform.system(), platform.machine()) != ('Linux', 'x86_64'):
        raise RuntimeError('This native build supports Linux x86_64; use a Linux x86_64 host or VM')
    for tool in ['git', 'rustup', 'cargo', 'rustc', 'node', 'curl', 'cc', 'ar']:
        if not shutil.which(tool):
            raise RuntimeError(f'Missing prerequisite: {tool}; see BUILDING.md')
    output, cache = args.output.resolve(), args.cache.resolve()
    if output.exists():
        raise RuntimeError(f'Output already exists: {output}; choose a new --output directory (the cache is reusable)')
    if output == cache or output.is_relative_to(cache) or cache.is_relative_to(output):
        raise RuntimeError('Output and cache must be separate directories')
    git = lambda *a: subprocess.check_output(['git', '-C', str(ROOT), *a], text=True).strip()
    if git('diff', 'HEAD', '--'):
        raise RuntimeError('Commit tracked source changes first so the build receipt identifies the source')
    revision = git('rev-parse', 'HEAD')
    channel = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    env = {k: v for k, v in os.environ.items() if not k.startswith(
        ('CARGO_', 'RUST', 'CC', 'CXX', 'AR', 'LD', 'RANLIB', 'CFLAGS', 'LIBSQLITE', 'WALLET_'))}
    env.update(CARGO_HOME=str(cache / 'cargo'), CARGO_TARGET_DIR=str(cache / 'target/wallet'),
               CARGO_BUILD_JOBS='2', RUSTUP_TOOLCHAIN=channel)

    def run(*command, cwd=ROOT, capture=False, data=None):
        print('+', ' '.join(map(str, command)), flush=True)
        return subprocess.run(list(map(str, command)), cwd=cwd, env=env, check=True,
                              text=True, input=data, stdout=subprocess.PIPE if capture else None).stdout

    # rustup owns the Rust version/target; the repository file is the sole version pin.
    run('rustup', 'toolchain', 'install', channel, '--profile', 'minimal', '--target', 'wasm32-unknown-unknown', '--no-self-update')
    if int(run('node', '-p', 'process.versions.node.split(".")[0]', capture=True).strip()) < 22:
        raise RuntimeError('Node 22 or newer is required')
    cache.mkdir(parents=True, exist_ok=True)
    tools = cache / 'tools'
    tools.mkdir(exist_ok=True)
    for name, (url, digest) in TOOLS.items():
        archive = tools / (name + '.tar.gz')
        if not archive.exists():
            partial = archive.with_suffix('.part')
            run('curl', '--fail', '--location', '--retry', '3', '--output', partial, url)
            if sha(partial) != digest:
                partial.unlink()
                raise RuntimeError(f'Tool download checksum mismatch: {name}')
            partial.rename(archive)
        if sha(archive) != digest:
            raise RuntimeError(f'Cached archive checksum mismatch: {archive}; remove it and retry')
        # Re-extract verified archives, so modified cached tools cannot enter a build.
        shutil.rmtree(tools / name, ignore_errors=True)
        with tarfile.open(archive) as tar:
            tar.extractall(tools, filter='data')
    sdk, bindgen_dir = (tools / name for name in TOOLS)
    bindgen = bindgen_dir / 'wasm-bindgen'
    if run(bindgen, '--version', capture=True).strip() != 'wasm-bindgen 0.2.128':
        raise RuntimeError('Unexpected wasm-bindgen version')

    # The two owned policy patches need their upstream archives before Cargo can
    # resolve the root manifest's path dependencies. Fetch outside that manifest.
    policy = json.loads((ROOT / 'native-policy/upstream.json').read_text())
    with tempfile.TemporaryDirectory(prefix='zakura-fetch-') as scratch:
        for name, package in policy.items():
            run('cargo', 'info', f"{name}@{package['version']}", cwd=scratch)
    vendor = ROOT / 'native-policy/vendor'
    receipt_path = vendor / 'receipt.json'
    prepared = json.loads(receipt_path.read_text()) if receipt_path.exists() else {}
    if set(prepared) != set(policy) or any(
            prepared[name]['upstream'] != package['checksum']
            or prepared[name]['patch'] != sha(ROOT / 'native-policy' / (name + '.patch'))
            for name, package in policy.items()):
        shutil.rmtree(vendor, ignore_errors=True)
        run(sys.executable, ROOT / 'native-policy/prepare.py')
    run('cargo', 'fetch', '--locked')
    output.mkdir(parents=True)
    metadata = json.loads(run('cargo', 'metadata', '--locked', '--offline', '--features', 'wallet-storage',
                              '--format-version', '1', capture=True))
    packages = json.loads(run(sys.executable, ROOT / 'scripts/verify-native-inputs.py', ROOT, output,
                              capture=True, data=json.dumps(metadata)))
    sqlite = next(p for p in metadata['packages'] if p['name'] == 'libsqlite3-sys')
    env.update(WALLET_SDK=str(sdk), WALLET_SQLITE=str(Path(sqlite['manifest_path']).parent / 'sqlite3'),
               CC_wasm32_unknown_unknown=str(sdk / 'bin/clang'), AR_wasm32_unknown_unknown=str(sdk / 'bin/llvm-ar'),
               CFLAGS_wasm32_unknown_unknown=f'--target=wasm32-wasi --sysroot={sdk}/share/wasi-sysroot -DSQLITE_OS_OTHER=1 -USQLITE_THREADSAFE -DSQLITE_THREADSAFE=0 -DSQLITE_TEMP_STORE=3 -DSQLITE_OMIT_LOAD_EXTENSION=1',
               LIBSQLITE3_FLAGS='-DSQLITE_ENABLE_MEMSYS5 -DSQLITE_ZERO_MALLOC -DLONGDOUBLE_TYPE=double -DSQLITE_OMIT_WAL',
               CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS=f'--remap-path-prefix={ROOT}=/source --remap-path-prefix={cache / "cargo"}=/cargo -C link-arg=--max-memory=268435456')
    run('cargo', 'test', '--locked', '--offline', '--lib')
    for name, features in [('primitive', []), ('bundle', ['--features', 'wallet-storage'])]:
        run('cargo', 'build', '--locked', '--offline', '--release', '--target', 'wasm32-unknown-unknown', '--lib', *features)
        run(bindgen, '--target', 'web', '--keep-lld-exports', '--out-dir', output / name, '--out-name', 'bindings',
            cache / 'target/wallet/wasm32-unknown-unknown/release/zakura_network_bindings.wasm')
        for module in ['bytes.mjs', 'network.mjs', 'transaction.mjs'] + (['wallet.mjs', 'views.mjs'] if name == 'bundle' else []):
            shutil.copyfile(ROOT / module, output / name / module)
    shutil.copytree(ROOT / 'wallet-host', output / 'bundle/wallet-host')
    run('node', 'tests/node.mjs', output / 'primitive')
    run('node', 'tests/transaction.mjs', output / 'primitive')
    inspection = json.loads(run('node', 'tests/wallet-inspect.mjs', output / 'bundle', capture=True))
    if not inspection['pass']:
        raise RuntimeError('Native wallet inspection failed')
    for name in ['lightwire', 'transparent-address']:
        run('cargo', 'fetch', '--locked', cwd=ROOT / name)
        run(sys.executable, ROOT / name / 'build.py', '--output', output / name,
            '--cargo-home', cache / 'cargo', '--bindgen', bindgen, '--target-dir', cache / 'target' / name)
    if git('diff', 'HEAD', '--') or git('rev-parse', 'HEAD') != revision:
        raise RuntimeError('Tracked sources changed during the build')
    receipt = dict(format='zcash-js-native-build/1', complete=True, revision=revision,
                   tree=git('rev-parse', 'HEAD^{tree}'), lockSha256=sha(ROOT / 'Cargo.lock'),
                   nativePolicy=json.loads((ROOT / 'native-policy/vendor/receipt.json').read_text()),
                   packages=packages, rustc=run('rustc', '-Vv', capture=True), producerSha256=sha(Path(__file__)),
                   bindgenSha256=sha(bindgen), sdkClangSha256=sha(sdk / 'bin/clang'),
                   inspection=inspection, artifacts=inventory(output / 'bundle'),
                   primitiveArtifacts=inventory(output / 'primitive'),
                   codecs={name: sha(output / name / 'receipt.json') for name in ['lightwire', 'transparent-address']})
    (output / 'build.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(f'Native SDK build complete: {output}', flush=True)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, OSError) as error:
        sys.exit(f'Native build failed: {error}')
