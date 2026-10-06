#!/usr/bin/env python3
"""Writes tools/cases.json: the inputs both the Go (godump) and Rust sides run.

For every registered bidder: each of its fixture requests as is (with the fixture's own mock
response), plus the first exemplary request in many mangled variants and with garbage responses.
Both sides get the same endpoint, extra_info and platform_id, taken from the seller's bidder-info.
"""
import glob, json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)

def bidder_names():
    s = open('src/registry.rs').read()
    return re.findall(r'^\s+"([^"]+)",\s*$', s.split('pub fn build')[0], re.M)

FIXDIR = {'emx_digital': 'cadent_aperture_mx', 'mediafuse': 'appnexus'}

def info(name):
    """endpoint / extra_info / platform_id / app_secret from the bidder-info yaml (flat keys only)."""
    out = {}
    for line in open(f'bidder-info/{name}.yaml'):
        m = re.match(r'^(endpoint|extra_info|platform_id|app_secret):\s*(.*?)\s*$', line)
        if m:
            v = m.group(2)
            if len(v) >= 2 and v[0] == v[-1] == '"':
                # YAML double-quoted scalar: the escapes are JSON's for everything used here.
                v = json.loads(v)
            elif len(v) >= 2 and v[0] == v[-1] == "'":
                v = v[1:-1].replace("''", "'")
            out[m.group(1)] = v
    return out

# Bidders with no endpoint in the seller's yaml get one from pbsBidderEndpointOverrides in prod.
NEEDS_ENDPOINT = {'adxcg', 'avocet', 'ix', 'pangle'}
# Go's own tests build these with their own config; the yaml values are placeholders or empty.
TEST_ENDPOINT = {}

def imp0(r):
    imps = r.get('imp')
    return imps[0] if isinstance(imps, list) and imps and isinstance(imps[0], dict) else None

def setdefault_imp(r, key, val):
    i = imp0(r)
    if i is not None:
        i[key] = val

def pop_imp(r, keys):
    i = imp0(r)
    if i is not None:
        for k in keys:
            i.pop(k, None)

def dup_imp(r):
    i = imp0(r)
    if i is not None:
        j = json.loads(json.dumps(i)); j['id'] = 'second-imp'; r['imp'].append(j)

def ext_bidder(r, val):
    i = imp0(r)
    if i is not None and isinstance(i.get('ext'), dict):
        i['ext']['bidder'] = val

VARIANTS = [
    ('as-is', lambda r: None),
    ('no imps', lambda r: r.__setitem__('imp', [])),
    ('imp null', lambda r: r.__setitem__('imp', None)),
    ('ext removed', lambda r: pop_imp(r, ['ext'])),
    ('ext {}', lambda r: setdefault_imp(r, 'ext', {})),
    ('ext null', lambda r: setdefault_imp(r, 'ext', None)),
    ('ext string', lambda r: setdefault_imp(r, 'ext', 'garbage')),
    ('ext.bidder {}', lambda r: ext_bidder(r, {})),
    ('ext.bidder []', lambda r: ext_bidder(r, [])),
    ('imp duplicated', dup_imp),
    ('no media type', lambda r: pop_imp(r, ['banner', 'video', 'native', 'audio'])),
    ('empty banner', lambda r: setdefault_imp(r, 'banner', {})),
    ('empty video', lambda r: setdefault_imp(r, 'video', {})),
    ('empty native', lambda r: setdefault_imp(r, 'native', {})),
    ('site removed', lambda r: r.pop('site', None)),
    ('app and site', lambda r: r.update(site={'id': 's', 'page': 'https://example.com/p', 'publisher': {'id': 'p'}}, app={'id': 'a', 'bundle': 'com.example.app', 'publisher': {'id': 'p'}})),
    ('device removed', lambda r: r.pop('device', None)),
    ('user removed', lambda r: r.pop('user', None)),
    ('gdpr+coppa', lambda r: r.update(regs={'coppa': 1, 'ext': {'gdpr': 1, 'us_privacy': '1YYY'}}, user={'ext': {'consent': 'CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA'}})),
    ('test+tmax0', lambda r: r.update(test=1, tmax=0)),
    ('cur eur jpy', lambda r: r.__setitem__('cur', ['EUR', 'JPY'])),
    ('unicode ids', lambda r: (r.__setitem__('id', 'id-ü-<b>&"\''), setdefault_imp(r, 'id', 'imp-日本-</script>'))),
]

GARBAGE = [
    ('empty200', 200, ''), ('null', 200, 'null'), ('{}', 200, '{}'), ('[]', 200, '[]'),
    ('html', 200, '<html>oops</html>'), ('truncated', 200, '{"id":"1","seatbid":[{"bid":['),
    ('empty seatbid', 200, '{"id":"1","seatbid":[]}'), ('empty bid', 200, '{"id":"1","seatbid":[{"bid":[{}]}]}'),
    ('wrong types', 200, '{"id":1,"seatbid":"x","cur":5}'),
    ('bid no impid', 200, '{"id":"1","seatbid":[{"bid":[{"id":"b","price":1.5,"adm":"<div/>"}]}]}'),
    ('unknown imp', 200, '{"id":"1","seatbid":[{"bid":[{"id":"b","impid":"nope","price":1.5}]}]}'),
    ('204', 204, ''), ('400', 400, 'bad'), ('500', 500, 'boom'),
]

def load(path):
    try:
        return json.load(open(path))
    except Exception:
        return None

cases = []
for name in bidder_names():
    inf = info(name)
    endpoint = inf.get('endpoint', '')
    if name in NEEDS_ENDPOINT:
        endpoint = f'https://{name}.example.test/openrtb'
    base_cfg = {'bidder': name, 'endpoint': endpoint, 'extra_info': inf.get('extra_info', ''),
                'platform_id': inf.get('platform_id', ''), 'app_secret': inf.get('app_secret', '')}
    fdir = FIXDIR.get(name, name)
    files = sorted(glob.glob(f'tests/fixtures/{fdir}/*/*.json'))
    first_exemplary = None
    for f in files:
        d = load(f)
        if not isinstance(d, dict) or 'mockBidRequest' not in d:
            continue
        kind = f.split('/')[-2]
        calls = d.get('httpCalls') or d.get('httpcalls') or []
        def to_resp(call):
            mr = call.get('mockResponse') or {}
            body = mr.get('body')
            # A fixture body is a `json.RawMessage`: the adapter receives its JSON text, so the
            # string `""` arrives as the two characters `""` (hence "found \"" in expected errors).
            # An absent body is `null` (a nil slice, which kobler and huaweiads tell from empty).
            return {'status': mr.get('status', 0), 'body': None if body is None else json.dumps(body, ensure_ascii=False, separators=(',', ':'))}
        base_id = f'{name}|fixture|{kind}/{os.path.basename(f)}'
        if not calls:
            cases.append({**base_cfg, 'id': base_id, 'request': d['mockBidRequest']})
        # One case per http call: `make_bids` runs against the request of the same index, as
        # Go's fixture runner does (a multi-call fixture has one response per request).
        for n, call in enumerate(calls):
            cases.append({**base_cfg, 'id': base_id if n == 0 else f'{base_id}#{n}', 'request': d['mockBidRequest'],
                          'response': to_resp(call), 'request_index': n})
        if first_exemplary is None and kind == 'exemplary':
            first_exemplary = d['mockBidRequest']
    if first_exemplary is None:
        continue
    for label, mangle in VARIANTS:
        r = json.loads(json.dumps(first_exemplary))
        mangle(r)
        cases.append({**base_cfg, 'id': f'{name}|variant|{label}', 'request': r})
    for label, status, body in GARBAGE:
        cases.append({**base_cfg, 'id': f'{name}|garbage|{label}', 'request': first_exemplary, 'response': {'status': status, 'body': body}})

json.dump(cases, open('tools/cases.json', 'w'))
print(len(cases), 'cases for', len({c['bidder'] for c in cases}), 'bidders', file=sys.stderr)
