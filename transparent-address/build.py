#!/usr/bin/env python3
"""Offline production build. Output must be a new directory outside the source tree."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tarfile, tomllib
ROOT = pathlib.Path(__file__).resolve().parent

def sha(data): return hashlib.sha256(data).hexdigest()
def snapshot(root):
    files = {}
    for p in sorted(root.rglob('*')):
        if p.is_symlink(): raise RuntimeError('source symlink: '+str(p))
        if p.is_file(): files[str(p.relative_to(root))] = sha(p.read_bytes())
    return files

def check_graph(metadata, cargo_home):
    lock = tomllib.loads((ROOT/'Cargo.lock').read_text())
    locked = {(p['name'],p['version']):p for p in lock['package']}
    records = []
    for p in metadata['packages']:
        source = p['source']
        if source is None:
            if pathlib.Path(p['manifest_path']).resolve() != ROOT/'Cargo.toml': raise RuntimeError('unexpected local package')
            continue
        if source != 'registry+https://github.com/rust-lang/crates.io-index': raise RuntimeError('unexpected registry source')
        selected = locked[(p['name'],p['version'])]
        root = pathlib.Path(p['manifest_path']).parent
        expected = cargo_home/'registry/src/index.crates.io-1949cf8c6b5b557f'/f"{p['name']}-{p['version']}"
        if root.resolve() != expected.resolve(): raise RuntimeError('unexpected cached source path')
        archive = cargo_home/'registry/cache/index.crates.io-1949cf8c6b5b557f'/f"{root.name}.crate"
        if sha(archive.read_bytes()) != selected['checksum']: raise RuntimeError('registry archive checksum mismatch')
        with tarfile.open(archive) as tar:
            members = {m.name.split('/',1)[1]:m for m in tar.getmembers() if m.isfile()}
            if any(f.is_symlink() for f in root.rglob('*')): raise RuntimeError('cached source symlink')
            actual = {str(f.relative_to(root)) for f in root.rglob('*') if f.is_file()} - {'.cargo-ok','.cargo-checksum.json'}
            if actual != set(members): raise RuntimeError('cached source inventory mismatch: '+root.name)
            for name,m in members.items():
                f = root/name
                if f.is_symlink() or f.read_bytes() != tar.extractfile(m).read(): raise RuntimeError('cached source mismatch: '+str(f))
        records.append({'name':p['name'],'version':p['version'],'source':source,'checksum':selected['checksum']})
    return records

def main():
    a=argparse.ArgumentParser();a.add_argument('--output',type=pathlib.Path,required=True);a.add_argument('--cargo-home',type=pathlib.Path,required=True);a.add_argument('--bindgen',type=pathlib.Path,required=True);a.add_argument('--target-dir',type=pathlib.Path);args=a.parse_args()
    out=args.output.resolve(); cargo_home=args.cargo_home.resolve(); target=(args.target_dir or out/'target').resolve()
    if out.is_relative_to(ROOT) or out.exists() or out.is_symlink(): raise RuntimeError('output must be a fresh external directory')
    out.mkdir(parents=True)
    receipt={'status':'incomplete','source':snapshot(ROOT)}
    receipt_path=out/'receipt.json'
    receipt_path.write_text(json.dumps(receipt,indent=2)+'\n')
    env=dict(os.environ,CARGO_HOME=str(cargo_home),CARGO_TARGET_DIR=str(target),CARGO_NET_OFFLINE='true')
    def run(name,cmd):
        with (out/(name+'.log')).open('wb') as log:
            subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        if snapshot(ROOT) != receipt['source']: raise RuntimeError('source mutated during '+name)
    run('rustc',['rustc','-vV']);run('cargo',['cargo','--version']);run('bindgen-version',[str(args.bindgen),'--version'])
    if (out/'bindgen-version.log').read_text().strip() != 'wasm-bindgen 0.2.128': raise RuntimeError('wrong wasm-bindgen version')
    receipt['bindgen_sha256']=sha(args.bindgen.read_bytes())
    receipt['tools']={}
    for tool in ['rustc','cargo']:
        path=pathlib.Path(subprocess.check_output(['rustup','which',tool],env=env,text=True).strip())
        receipt['tools'][str(path)]=sha(path.read_bytes())
    sysroot=pathlib.Path(subprocess.check_output(['rustc','--print','sysroot'],env=env,text=True).strip())
    receipt['target_libraries']={str(p):sha(p.read_bytes()) for p in sorted((sysroot/'lib/rustlib/wasm32-unknown-unknown/lib').glob('*')) if p.is_file()}
    if not receipt['target_libraries']: raise RuntimeError('missing installed wasm target libraries')
    run('metadata',['cargo','metadata','--locked','--offline','--format-version','1','--filter-platform','wasm32-unknown-unknown'])
    metadata=json.loads((out/'metadata.log').read_text()); receipt['graph']=check_graph(metadata,cargo_home)
    receipt['metadata_sha256']=sha((out/'metadata.log').read_bytes())
    receipt['graph_sha256']=sha(json.dumps(metadata['resolve'],sort_keys=True,separators=(',',':')).encode())
    run('native',['cargo','test','--locked','--offline'])
    run('wasm',['cargo','build','--locked','--offline','--release','--target','wasm32-unknown-unknown'])
    wasm=target/'wasm32-unknown-unknown/release/zakura_transparent_address.wasm'
    if not wasm.is_file(): raise RuntimeError('producing build did not create WASM')
    run('bindgen',[str(args.bindgen),'--target','web','--out-dir',str(out/'wasm'),str(wasm)])
    shutil.copy2(ROOT/'codec.mjs',out/'codec.mjs')
    receipt['raw_wasm_sha256']=sha(wasm.read_bytes())
    receipt['artifacts']={str(p.relative_to(out)):sha(p.read_bytes()) for p in [out/'codec.mjs',*sorted((out/'wasm').iterdir())] if p.is_file()}
    if snapshot(ROOT) != receipt['source']: raise RuntimeError('source mutated before completion')
    check_graph(metadata,cargo_home)
    receipt['status']='built';receipt_path.write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps({'status':'built','receipt':str(receipt_path),'artifacts':receipt['artifacts']},indent=2))
if __name__ == '__main__': main()
