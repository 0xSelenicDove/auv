import json,re,collections,hashlib,shutil
from pathlib import Path
r=Path(__file__).resolve().parent;p=Path('docs/ai/references/driver/evidence/skill-token-fix-benchmark');p.mkdir(parents=True,exist_ok=True)
o=json.loads((r/'oracle.json').read_text());truth={x['label']:x for x in o['rows']};metrics=[];failures=[];norm=lambda s:re.sub(r'[^a-z0-9]','',s.lower())
def operation_outputs(text):
 decoder=json.JSONDecoder();position=0
 while position<len(text):
  start=text.find('{',position)
  if start<0:return
  try:value,position=decoder.raw_decode(text,start)
  except json.JSONDecodeError:position=start+1;continue
  if isinstance(value,dict) and 'command_id' in value:yield value
for n in range(1,5):
 result=json.loads((r/f'trial-{n:02d}.result.json').read_text());audit=json.loads((r/f'trial-{n:02d}.audit.json').read_text());ims={x['path']:x for x in map(json.loads,(r/f'trial-{n:02d}.image-audit.jsonl').read_text().splitlines())}
 events=[json.loads(s) for s in (r/f'trial-{n:02d}.jsonl').read_text().splitlines()];cs=[e['item'] for e in events if e.get('type')=='item.completed' and e.get('item',{}).get('type')=='command_execution'];outputs=[d for c in cs for d in operation_outputs(c.get('aggregated_output',''))]
 receipts={a['file_path']:a for d in outputs for a in d.get('artifacts',[]) if 'file_path' in a};pics=[]
 for j,record in enumerate(result['answer']['records'],1):
  src=Path(record['evidence_path']);wanted=truth[record['name']];expected=norm(record['name']+' '+wanted['code']+' '+wanted['status']);im=ims[str(src)]
  assert any(expected in norm(row) for row in im['rows']),(n,record,im)
  digest=hashlib.sha256(src.read_bytes()).hexdigest();assert str(src) in receipts,(n,str(src));assert receipts[str(src)]['sha256']==digest
  dst=f'trial-{n:02d}-record-{j:02d}{src.suffix}';shutil.copyfile(src,p/dst);pics.append({'name':record['name'],'file':dst,'sha256':digest,'width':im['width'],'height':im['height'],'complete_row_on_one_ocr_line':True,'original_receipt_hash_matches':True})
 final=f'trial-{n:02d}.verify.jpg';shutil.copyfile(r/final,p/final)
 for suffix in ['prompt.txt','audit.json','result.json']:shutil.copyfile(r/f'trial-{n:02d}.{suffix}',p/f'trial-{n:02d}.{suffix}')
 u=result['usage'];metric={k:result[k] for k in ['trial','arm','case','seconds_including_startup','exit_code','timed_out','usage']};metric.update({'total_tokens':u['input_tokens']+u['output_tokens'],'uncached_input_plus_output':u['input_tokens']-u['cached_input_tokens']+u['output_tokens'],'verified_records':audit['verified_records'],'complete':audit['complete'],'completed_shell_calls':len(cs),'operation_result_count':len(outputs),'operation_counts':dict(collections.Counter(d['command_id'] for d in outputs)),'task_owned_daemon_started':any(' serve ' in c['command'] for c in cs),'failed_operations':sum(d['status']=='failed' for d in outputs),'evidence':pics,'final_image':final});metrics.append(metric)
 for d in outputs:
  if d['status']=='failed':failures.append({'trial':n,'response':d})
agg={}
for arm in ['before','after']:
 rows=[x for x in metrics if x['arm']==arm];agg[arm]={'seconds':sum(x['seconds_including_startup'] for x in rows),'total_tokens':sum(x['total_tokens'] for x in rows),'uncached_input_plus_output':sum(x['uncached_input_plus_output'] for x in rows),'verified_records':sum(x['verified_records'] for x in rows),'complete_tasks':sum(x['complete'] for x in rows)}
delta={k:(agg['after'][k]/agg['before'][k]-1)*100 for k in ['seconds','total_tokens','uncached_input_plus_output']}
(p/'metrics.json').write_text(json.dumps({'trials':metrics,'aggregate':agg,'change_percent':delta},indent=2)+'\n');(p/'failed-operation-responses.json').write_text(json.dumps(failures,indent=2)+'\n')
for name in ['manifest.json','protocol.md','manage.py','audit.py','oracle.json','publish.py']:shutil.copyfile(r/name,p/name)
shutil.copyfile('docs/notes/post-sync-benchmark/audit-images.swift',p/'audit-images.swift')
manifest=json.loads((r/'manifest.json').read_text())
for arm in ['before','after']:
 for file,key in [('bin/'+arm,'binary_sha256'),(arm+'/SKILL.md','skill_sha256'),(arm+'/references/operations.md','operations_sha256')]:assert hashlib.sha256((r/file).read_bytes()).hexdigest()==manifest['versions'][arm][key]
 shutil.copytree(r/arm,p/arm,dirs_exist_ok=True)
for name in ['SKILL.md','references/operations.md']:
 src=Path('.agents/skills/auv-computer-control')/name;assert src.read_bytes()==(r/'after'/name).read_bytes();shutil.copyfile(src,Path('/Users/hutao/.codex/skills/auv-computer-control')/name)
print(json.dumps({'aggregate':agg,'change_percent':delta,'trials':[{k:x[k] for k in ['trial','arm','case','seconds_including_startup','total_tokens','complete','completed_shell_calls','operation_counts','task_owned_daemon_started','failed_operations']} for x in metrics]},indent=2))
print('32 full rows and original hashes verified; frozen files unchanged; installed skill synced.')
print('Temporary authentication copies remaining:',len(list(Path('/tmp/auv-skill-token-fix-benchmark-20261008').glob('trial-*/codex-home/auth.json'))))
