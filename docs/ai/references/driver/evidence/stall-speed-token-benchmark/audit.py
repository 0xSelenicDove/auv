import json,re,subprocess,sys
from pathlib import Path
r=Path(__file__).resolve().parent;i=int(sys.argv[1]);result=json.loads((r/f'trial-{i:02d}.result.json').read_text());oracle=json.loads((r/'oracle.json').read_text());targets=oracle[result['case']];truth={row['label']:row for row in oracle['rows']};answer=result.get('answer') or {};records=answer.get('records',[]);trial=Path(f'/tmp/auv-stall-speed-token-benchmark-20261008/trial-{i:02d}');paths=[]
for row in records:
 p=Path(row.get('evidence_path',''))
 if p.resolve().is_relative_to(trial.resolve()) and p.is_file(): paths.append(str(p))
final=r/f'trial-{i:02d}.verify.jpg'
if final.is_file(): paths.append(str(final))
raw=subprocess.check_output([str(Path('/Users/hutao/github/auv/docs/notes/post-sync-benchmark/audit-images')),*dict.fromkeys(paths)],text=True);(r/f'trial-{i:02d}.image-audit.jsonl').write_text(raw);images={row['path']:row for row in map(json.loads,raw.splitlines())};norm=lambda s:re.sub(r'[^a-z0-9]','',s.lower());checks=[]
for target in targets:
 row=next((x for x in records if x.get('name')==target),{});wanted=truth[target];image=images.get(row.get('evidence_path'),{});text=norm(' '.join(image.get('rows',[])));expected=norm(target+' '+wanted['code']+' '+wanted['status']);checks.append({'name':target,'oracle_matches':row.get('code')==wanted['code'] and row.get('status')==wanted['status'],'evidence_contains_exact_row':expected in text,'subject_claims_inspected':row.get('screenshot_verified') is True,'evidence_width':image.get('width'),'evidence_height':image.get('height')})
wanted=truth[targets[-1]];finaltext=norm(' '.join(images.get(str(final),{}).get('rows',[])));visible=norm(targets[-1]+' '+wanted['code']+' '+wanted['status']) in finaltext
out={'trial':i,'record_checks':checks,'final_target_visible':visible,'verified_records':sum(x['oracle_matches'] and x['evidence_contains_exact_row'] and x['subject_claims_inspected'] for x in checks),'complete':answer.get('complete') is True and visible and all(x['oracle_matches'] and x['evidence_contains_exact_row'] and x['subject_claims_inspected'] for x in checks)};(r/f'trial-{i:02d}.audit.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({k:v for k,v in out.items() if k!='record_checks'}))
