import json, sys, time, uuid
from pathlib import Path

action_schema = {
    'type': 'object',
    'properties': {
        'action': {'type': 'string', 'enum': ['observe', 'click', 'scroll', 'set_value', 'key']},
        'element': {'type': 'integer', 'minimum': 0},
        'direction': {'type': 'string', 'enum': ['up', 'down']},
        'pages': {'type': 'number', 'minimum': 0.1, 'maximum': 5},
        'value': {'type': 'string'}, 'key': {'type': 'string'},
    },
    'required': ['action'], 'additionalProperties': False,
    'allOf': [
        {'if': {'properties': {'action': {'enum': ['click', 'scroll', 'set_value']}}},
         'then': {'required': ['element']}},
        {'if': {'properties': {'action': {'const': 'scroll'}}}, 'then': {'required': ['direction']}},
        {'if': {'properties': {'action': {'const': 'set_value'}}}, 'then': {'required': ['value']}},
        {'if': {'properties': {'action': {'const': 'key'}}}, 'then': {'required': ['key']}},
    ],
}
schema = {'type': 'object', 'properties': {'actions': {'type': 'array', 'items': action_schema,
          'minItems': 1, 'maxItems': 40}}, 'required': ['actions'], 'additionalProperties': False}

def validate(args):
    if set(args) != {'actions'} or not isinstance(args['actions'], list) or not 1 <= len(args['actions']) <= 40:
        raise ValueError('Supply actions: an array of 1..40 operations.')
    for a in args['actions']:
        if not isinstance(a, dict) or set(a) - set(action_schema['properties']):
            raise ValueError('Unknown action fields.')
        kind = a.get('action')
        if kind not in action_schema['properties']['action']['enum']:
            raise ValueError('Unsupported action.')
        if kind in ('click', 'scroll', 'set_value'):
            if type(a.get('element')) is not int or a['element'] < 0:
                raise ValueError('An observed element index is required for click, scroll and set_value.')
        if kind == 'scroll':
            pages = a.get('pages', 1)
            if a.get('direction') not in ('up', 'down') or type(pages) not in (int, float) or not 0.1 <= pages <= 5:
                raise ValueError('Scroll requires up/down and pages in 0.1..5.')
        if kind in ('set_value', 'key') and not isinstance(a.get('value' if kind == 'set_value' else 'key'), str):
            raise ValueError('Action requires a string value/key.')
    return args

def serve(root):
    root.mkdir(parents=True, exist_ok=True)
    for line in sys.stdin:
        req = json.loads(line); rid = req.get('id')
        if rid is None: continue
        try:
            method = req.get('method')
            if method == 'initialize':
                result = {'protocolVersion': req['params']['protocolVersion'], 'capabilities': {'tools': {}},
                          'serverInfo': {'name': 'bound-repeated-search', 'version': '2'}}
            elif method == 'tools/list':
                result = {'tools': [{'name': 'native_ui', 'inputSchema': schema,
                    'description': 'Native computer use bound ONLY to AUV Repeated Search - Synthetic Benchmark. '
                    'Batch deterministic actions freely. observe returns fresh AX state and an unmodified window screenshot '
                    'with evidence_path. Other actions return fresh AX state. Use observed element indices: click/scroll/set_value '
                    'require element. scroll also requires direction and permits pages 0.1..5 (default 1). '
                    'set_value accepts a string for an observed settable AX control, including scrollbar. '
                    'key is a named key sent only to this app. Records are drawn on a canvas, absent from AX text. '
                    'Use observe after actions to read pixels. The entire batch is validated before any action.'}]}
            elif method == 'tools/call':
                args = validate(req['params']['arguments']); token = uuid.uuid4().hex
                (root / f'{token}.request.json').write_text(json.dumps(args))
                response = root / f'{token}.response.json'; deadline = time.monotonic() + 115
                while not response.exists() and time.monotonic() < deadline: time.sleep(0.03)
                result = json.loads(response.read_text()) if response.exists() else {
                    'isError': True, 'content': [{'type': 'text', 'text': 'Native relay timed out; stop.'}]}
            else: result = {}
            print(json.dumps({'jsonrpc': '2.0', 'id': rid, 'result': result}), flush=True)
        except Exception as e:
            print(json.dumps({'jsonrpc': '2.0', 'id': rid, 'result': {
                'isError': True, 'content': [{'type': 'text', 'text': str(e)}]}}), flush=True)

if __name__ == '__main__':
    if sys.argv[1:] == ['--self-check']:
        validate({'actions': [{'action': 'scroll', 'element': 1, 'direction': 'down', 'pages': 5}]})
        for a in ({'action': 'scroll', 'direction': 'down'},
                  {'action': 'scroll', 'element': 1, 'direction': 'down', 'pages': 6}):
            try: validate({'actions': [a]})
            except ValueError: pass
            else: raise AssertionError('Invalid scroll accepted')
        print('Native missing-element and page-bound regression checks passed.')
    else: serve(Path(sys.argv[1]))
