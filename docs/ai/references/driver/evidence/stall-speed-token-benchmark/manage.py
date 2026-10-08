import json,os,shutil,subprocess,time,signal,sys,hashlib
from pathlib import Path
r=Path(__file__).resolve().parent;root=Path('/tmp/auv-stall-speed-token-benchmark-20261008');n=int(sys.argv[1]);trial=root/f'trial-{n:02d}';home=trial/'codex-home';home.mkdir(parents=True,exist_ok=True);arm=['before','after','after','before'][n-1];case='A' if n<=2 else 'B';queue=trial/'queue';queue.mkdir(exist_ok=True)
source=Path(os.environ.get('CODEX_HOME',str(Path.home()/'.codex')))
shutil.copyfile(source/'auth.json',home/'auth.json');(home/'auth.json').chmod(0o600)
if arm in ('before','after'):shutil.copytree(r/arm,home/'skills/auv-computer-control',dirs_exist_ok=True)
config='web_search = "disabled"\n'+f'[projects.{json.dumps(str(trial.resolve()))}]\ntrust_level = "trusted"\n'
if arm=='native':config+='\n[tools]\nshell = false\n\n[mcp_servers.native]\ncommand = "python3"\nargs = '+json.dumps([str(r/'native_mcp.py'),str(queue)])+'\ntool_timeout_sec = 120\n'
if arm in ('before','after'):config+='\n[[skills.config]]\npath = '+json.dumps(str(home/'skills/auv-computer-control/SKILL.md'))+'\nenabled = false\n'
(home/'config.toml').write_text(config)
args=['codex','exec','--skip-git-repo-check','--ephemeral','--json','--color','never','-m','gpt-5.6-sol','-c','model_reasoning_effort="low"','-c','approval_policy="never"','-c','sandbox_mode="danger-full-access"','-c','features.apps=false','-c','features.plugins=false','-c','features.multi_agent=false','-c','features.browser_use=false','-c','features.computer_use=false',*(['-c','features.shell_tool=false'] if arm=='native' else []),'-C',str(trial),'-o',str(trial/'answer.txt'),'-']
prompt=(r/f'trial-{n:02d}.prompt.txt').read_text();start=time.monotonic();timed_out=False
out=(r/f'trial-{n:02d}.jsonl').open('w');err=(r/f'trial-{n:02d}.stderr').open('w')
try:
 p=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=out,stderr=err,env={**os.environ,'CODEX_HOME':str(home)},start_new_session=True)
 (r/'active.json').write_text(json.dumps({'trial':n,'arm':arm,'case':case,'root':str(trial),'pid':p.pid}))
 p.stdin.write(prompt.encode());p.stdin.close()
 try:code=p.wait(timeout=900)
 except subprocess.TimeoutExpired:
  timed_out=True;os.killpg(p.pid,signal.SIGTERM)
  try:code=p.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);code=p.wait()
 elapsed=time.monotonic()-start
finally:
 out.close();err.close();(home/'auth.json').unlink(missing_ok=True)
events=[]
for line in (r/f'trial-{n:02d}.jsonl').read_text().splitlines():
 try:events.append(json.loads(line))
 except ValueError:pass
usage={}
for e in events:
 if e.get('type')=='turn.completed':
  for k,v in e.get('usage',{}).items():usage[k]=usage.get(k,0)+v
answer=None;text=(trial/'answer.txt').read_text() if (trial/'answer.txt').exists() else ''
try:answer=json.loads(text.strip().removeprefix('```json').removeprefix('```').removesuffix('```').strip())
except ValueError:pass
result={'trial':n,'arm':arm,'case':case,'seconds_including_startup':elapsed,'exit_code':code,'timed_out':timed_out,'usage':usage,'answer':answer,'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest()};(r/f'trial-{n:02d}.result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='answer'}))
# Kill only descendants left in this measured session's own process group.
try:os.killpg(p.pid,signal.SIGTERM)
except ProcessLookupError:pass
