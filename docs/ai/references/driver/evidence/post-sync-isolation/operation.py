import json,subprocess,sys,time
from pathlib import Path
r=Path(__file__).resolve().parent
label=sys.argv[1];version=sys.argv[2];mode=sys.argv[3];args=sys.argv[4:]
bin=Path('docs/notes/repeated-search-benchmark/bin/auv' if version=='old' else 'docs/notes/post-sync-benchmark/bin/auv').resolve()
trial=r/label;trial.mkdir(exist_ok=True)
prefix=[str(bin)];env=None
if mode=='runner':
 devices=json.loads((r/(version+'-devices.json')).read_text()); device=next(d for d in devices['devices'] if d.get('local'))
 prefix += ['--endpoint','unix://'+str(r/(version+'.sock')),'--device-id',device.get('device_id',device.get('id'))]
command=prefix+['invoke',*args,'--target','app:local.auv.RepeatedSearchFixture','--title','AUV Repeated Search - Synthetic Benchmark','--store-root',str(trial/'store'),'--compact-json']
# app.activate has no window-title argument.
if args[0]=='app.activate':
 j=command.index('--title');del command[j:j+2]
start=time.monotonic();p=subprocess.run(command,capture_output=True,text=True,timeout=60);elapsed=time.monotonic()-start
(trial/'stdout.json').write_text(p.stdout);(trial/'stderr.txt').write_text(p.stderr)
try:out=json.loads(p.stdout)
except ValueError:out={}
row={'label':label,'version':version,'mode':mode,'arguments':args,'seconds':elapsed,'exit_code':p.returncode,'output':out};(trial/'measurement.json').write_text(json.dumps(row,indent=2)+'\n');print(json.dumps(row))
