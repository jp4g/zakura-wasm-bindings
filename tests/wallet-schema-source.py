"""Check the frozen, source-located DDL projection. This is NOT its completeness proof.

Reachable SQL blocks and exceptional branches are audited in src/wallet/schema.md.
Source changes fail closed and require a new audit; this does not infer a contract
from arbitrary Rust using regular expressions. SQL runs only in the native test.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
manifest = json.loads(Path(__file__).with_name('wallet-schema-source.json').read_text())
# These are pinned producer codes, not values inferred from wallet observations.
codes = dict(account_kind_derived='0', account_kind_imported='1', sapling_pool_code='2',
             orchard_pool_code='3', transparent_pool_code='0', LEGACY_ADDRESS_INDEX_NULL='-1',
             foreign_key_scope='-1', external_scope_code='0', fallback_height='0')
literals = re.compile(r'//[^\n]*|/\*.*?\*/|r(#+)".*?"\1|"(?:[^"\\]|\\.)*"', re.S)

def split_sql(sql):
    # Semicolon-aware lexical split, preserving all bytes including literals.
    token = re.compile(r"--[^\n]*|/\*.*?\*/|'(?:''|[^'])*'|\"(?:\"\"|[^\"])*\"|;", re.S)
    start = 0
    for m in token.finditer(sql):
        if m[0] == ';':
            yield sql[start:m.end()]
            start = m.end()
    if sql[start:].strip(): yield sql[start:]+';'

for item in manifest:
    path = root/(item['name']+'.rs')
    assert hashlib.sha256(path.read_bytes()).hexdigest() == item['source_sha256'], path
    text = path.read_text().split('#[cfg(test)]')[0].split('fn down(')[0]
    id_ = re.search(r'MIGRATION_ID: Uuid = Uuid::from_u128\(0x([0-9a-f_]+)\)', text)[1].replace('_','')
    assert id_ == item['id']
    deps = re.search(r'const DEPENDENCIES:.*?= &\[(.*?)\];', text, re.S)
    names = re.findall(r'(\w+)::MIGRATION_ID', deps[1]) if deps else []
    by_name = {m['name']:m['id'] for m in manifest}
    assert [by_name[name] for name in names] == item['dependencies'], (path, 'dependencies differ')
    found = {}
    for match in literals.finditer(text):
        raw = match[0]
        if raw.startswith(('//','/*')): continue
        if raw.startswith('r'): sql = raw[raw.index('"')+1:raw.rindex('"')]
        else:
            try: sql = json.loads(raw.replace('\n','\\n'))
            except ValueError: continue
        if re.search(r'\b(CREATE|DROP|ALTER)\s+(TABLE|TEMPORARY|VIEW|INDEX|UNIQUE)', sql):
            found[text[:match.start()].count('\n')+1] = sql
    assert sorted(found) == item['source_sql_lines'], (path, 'source block coverage changed')
    statements = []
    for line, sql in found.items():
        name = item['name']
        if name == 'orchard_ironwood_migration_anchor_interval':
            # RC4 CREATE_TABLES_SQL already has this column/default. Its branch
            # tests column presence; it does not add it a second time.
            continue
        if '{}' in sql:
            if 'shard_scan_ranges AS' in sql:
                values = [16,16,dict(orchard_shardtree=60, ironwood_shardtree=100,
                                    v_sapling_shard_unscanned_ranges=20)[name]]
            elif 'shard_unscanned_ranges AS' in sql: values = [10]
            else: values = [20 if name == 'add_account_birthdays' else 0]
            for value in values: sql = sql.replace('{}',str(value),1)
        for key, value in codes.items(): sql = sql.replace('{'+key+'}',value)
        for statement in split_sql(sql):
            # Only classify the leading SQL keyword; never normalize the stored SQL.
            bare = re.sub(r'^(?:\s|--[^\n]*|/\*.*?\*/)*', '', statement, flags=re.S)
            if re.match(r'(CREATE (?!TEMP)|ALTER |DROP |PRAGMA legacy_alter_table)',bare):
                assert '{' not in statement
                statements.append(dict(line=line,sql=statement))
    if item['name'] == 'orchard_ironwood_migration_unsatisfiability':
        # The module-level constant is invoked after the three column renames.
        statements.sort(key=lambda s: {256:0,105:1,278:2,353:3}[s['line']])
    assert statements == item['statements'], (path, 'source projection differs')
print(json.dumps(dict(source_migrations=66, projection_sha256=hashlib.sha256(
    Path(__file__).with_name('wallet-schema-source.json').read_bytes()).hexdigest())))
