#!/usr/bin/env python3
"""Failure controls: source mutation never produces a successful build receipt."""
import json, os, pathlib, shutil, subprocess, tempfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
base=pathlib.Path(os.environ.get('LIGHTWIRE_SCRATCH','/home/jack/zakura-lightwire-scratch'))
run=pathlib.Path(tempfile.mkdtemp(prefix='build-controls-',dir=base))
source=run/'source';shutil.copytree(ROOT,source)
injector=run/'bindgen-mutation'
injector.write_text('#!/usr/bin/env python3\nfrom pathlib import Path\nPath("codec.mjs").write_text("// mutated\\n")\nprint("wasm-bindgen 0.2.128")\n');injector.chmod(0o700)
output=run/'output'
command=['python3','-O',str(source/'build.py'),'--output',str(output),'--cargo-home',str(base/'cargo'),'--bindgen',str(injector)]
result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=30)
(run/'mutation.log').write_text(result.stdout)
if result.returncode==0 or 'source mutated during bindgen-version' not in result.stdout:raise RuntimeError('mutation control did not fail correctly')
receipt=json.loads((output/'receipt.json').read_text())
if receipt['status']!='incomplete' or 'artifacts' in receipt or (output/'wasm').exists():raise RuntimeError('failure published a success artifact')
# Refuse reuse rather than deleting a caller-owned directory.
sentinel=output/'sentinel';sentinel.write_text('keep')
result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=30)
if result.returncode==0 or sentinel.read_text()!='keep':raise RuntimeError('output reuse safety failed')
# Source deletion is visible in the exact inventory, and symlinks are rejected.
namespace={'__name__':'build_helpers','__file__':str(ROOT/'build.py')};exec(compile((ROOT/'build.py').read_text(),str(ROOT/'build.py'),'exec'),namespace)
before=namespace['snapshot'](source);(source/'codec.mjs').unlink()
if namespace['snapshot'](source)==before:raise RuntimeError('missing source not detected')
(source/'codec.mjs').symlink_to(ROOT/'codec.mjs')
try:namespace['snapshot'](source)
except RuntimeError:pass
else:raise RuntimeError('source symlink accepted')
print(json.dumps({'ok':True,'checks':4,'optimized_python':True,'run':str(run)}))
