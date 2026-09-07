#!/usr/bin/env python3
import os, subprocess, json, time, tempfile
from pathlib import Path
import argparse
parser=argparse.ArgumentParser(description='Record preview only using Hyprland, grim and ydotool. Moves the pointer for about 30 seconds.')
parser.add_argument('video', type=Path)
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
cache=Path.home()/'.cache/decklock'
cache.mkdir(parents=True, exist_ok=True)
frames=Path(tempfile.mkdtemp(prefix='demo-', dir=cache)); command=frames/'pointer'
env=dict(os.environ, LD_LIBRARY_PATH=str(root/'.deps/install/lib'))
log=open(frames/'preview.log','w')
p=subprocess.Popen([str(root/'target/debug/examples/record_demo'),str(args.video.resolve()),str(command)],env=env,stdout=log,stderr=log)
def run(args):
 subprocess.run(args,check=True,stdout=subprocess.DEVNULL)
try:
 time.sleep(1)
 c=next(c for c in json.loads(subprocess.check_output(['hyprctl','clients','-j'])) if c['pid']==p.pid)
 x,y=c['at'];w,h=c['size'];start=time.monotonic();previous=''
 for n in range(175):
  if p.poll() is not None: break
  click=False
  if command.exists():
   msg=command.read_text()
   if msg and msg!=previous:
    previous=msg;step,px,py,down=msg.split()
    run(['hyprctl','dispatch','movecursor',f'{x+int(float(px))} {y+int(float(py))}'])
    run(['ydotool','mousemove','--','1','0'])
    time.sleep(0.04)
    click=down=='1'
    if click: run(['ydotool','click','0x40']);time.sleep(0.06)
  run(['grim','-c','-l','0','-s','0.5','-g',f'{x},{y} {w}x{h}',str(frames/f'{n:04}.png')])
  if click: run(['ydotool','click','0x80'])
  time.sleep(max(0,start+(n+1)/8-time.monotonic()))
 print(frames,flush=True)
finally:
 run(['ydotool','click','0x80'])
 if p.poll() is None:p.terminate()
 p.wait(timeout=5)
 log.close()
