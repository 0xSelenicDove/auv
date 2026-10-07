import subprocess,json,time,sys,hashlib
from pathlib import Path
r=Path(__file__).resolve().parent
i=int(sys.argv[1]);profile=['debug','release','release','debug'][i-1]
binary=Path('/Users/hutao/github/auv/docs/notes/scroll-routing-benchmark/auv') if profile=='debug' else Path('/Users/hutao/github/auv/target/release/auv')
for bundle in ['local.auv.CanvasFixture','local.auv.ScrollCover']:
 d=json.loads(subprocess.check_output([str(binary),'invoke','window.list','--target','app:'+bundle,'--compact-json'],text=True));assert len(d['result'])==1,(bundle,d)
args=[str(binary),'invoke','input.scrollUntil','450','350','--dy','420','--until','text:Kestrel handoff','--max-steps','40','--settle-ms','300','--target','app:local.auv.CanvasFixture','--title','AUV Canvas Ledger - Synthetic Benchmark','--input-policy','background-only','--store-root',str(r/f'run-{i}'),'--compact-json']
start=time.perf_counter();p=subprocess.run(args,capture_output=True,text=True,timeout=180);elapsed=time.perf_counter()-start
(r/f'run-{i}.stdout.json').write_text(p.stdout);(r/f'run-{i}.stderr').write_text(p.stderr)
d=json.loads(p.stdout);result=d['result']['result'];row={'run':i,'profile':profile,'seconds':elapsed,'exit_code':p.returncode,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'steps':result['steps'],'reason':result['reason'],'delivered':result['delivered'],'text_match':result.get('text_match'),'action':result['action'],'artifacts':d['artifacts']}
(r/f'run-{i}.metrics.json').write_text(json.dumps(row,indent=2)+'\n');print(json.dumps(row),flush=True)
assert p.returncode==0 and result['reason']=='text_visible'
