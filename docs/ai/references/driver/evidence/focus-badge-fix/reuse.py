import json,subprocess,time,signal,os
from pathlib import Path
r=Path(__file__).resolve().parent;b=Path('target/release/auv').resolve();endpoint='unix://'+str(r/'measured.sock')
log=(r/'daemon.log').open('w');start=time.monotonic();daemon=subprocess.Popen([str(b),'serve','--listen',endpoint,'--store-root',str(r/'measured-daemon'),'--no-register'],stdout=log,stderr=log)
rows=[]
try:
 deadline=start+10
 while True:
  if daemon.poll() is not None:raise RuntimeError('daemon exited')
  p=subprocess.run([str(b),'devices','list','--endpoint',endpoint,'--json'],capture_output=True,text=True)
  if p.returncode==0:
   devices=json.loads(p.stdout);device=next(x['device_id'] for x in devices if x['local']);break
  if time.monotonic()>deadline:raise RuntimeError('daemon startup timeout')
  time.sleep(.05)
 startup=time.monotonic()-start
 (r/'device.json').write_text(p.stdout)
 for i,mode in enumerate(['direct','runner','runner','direct'],1):
  prefix=[str(b)]
  if mode=='runner':prefix+=['--device-id',device]
  args=prefix+['invoke','window.findText','Kestrel handoff','--target','app:local.auv.CanvasFixture','--title','AUV Canvas Ledger - Synthetic Benchmark','--store-root',str(r/f'ocr-{i}-store'),'--compact-json','--no-overlay']
  t=time.monotonic()
  try:
   p=subprocess.run(args,capture_output=True,text=True,timeout=120,env={**os.environ, **({'AUV_ENDPOINT':endpoint} if mode=='runner' else {})});row={'attempt':i,'mode':mode,'seconds':time.monotonic()-t,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
  except subprocess.TimeoutExpired as e:
   row={'attempt':i,'mode':mode,'seconds':time.monotonic()-t,'exit_code':None,'timeout':True,'stdout':(e.stdout or b'').decode(),'stderr':(e.stderr or b'').decode()}
  rows.append(row);(r/'reuse.json').write_text(json.dumps({'startup_seconds':startup,'order':['direct','runner','runner','direct'],'rows':rows},indent=2)+'\n');print(json.dumps({k:v for k,v in row.items() if k not in ['stdout','stderr']}),flush=True)
finally:
 daemon.send_signal(signal.SIGINT)
 try:daemon.wait(timeout=10)
 except subprocess.TimeoutExpired:daemon.kill();daemon.wait()
 log.close()
