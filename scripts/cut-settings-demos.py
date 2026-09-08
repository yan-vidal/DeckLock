#!/usr/bin/env python3
"""Cut short native-1080p README demos from the recorder's video and event timings."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('video',type=Path)
parser.add_argument('events',type=Path)
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
events={int(k):v for k,v in json.loads(args.events.read_text()).items()}
with tempfile.TemporaryDirectory(prefix='decklock-clips-') as work:
    work=Path(work)
    clips=[('settings-themes',events[1]-.3,events[2]+2.4),
           ('settings-library',events[11],events[12]+4.2),
           ('settings-rest',events[5],events[7]+2)]
    def encode(source, name, start=0, duration=None):
        base=['ffmpeg','-v','error','-y','-ss',str(start)]
        if duration is not None:base+=['-t',str(duration)]
        base+=['-i',str(source)]
        palette=work/(name+'.png')
        subprocess.run(base+['-vf','fps=8,palettegen=max_colors=192','-frames:v','1',str(palette)],check=True)
        subprocess.run(base+['-i',str(palette),'-lavfi','fps=8[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3','-loop','0',str(root/'docs/assets'/(name+'.gif'))],check=True)
    for name,start,end in clips:encode(args.video,name,start,end-start)
    short=work/'editor.mp4'
    cuts=f'[0:v]trim=start={events[15]-.5}:end={events[16]+.3},setpts=PTS-STARTPTS[a];[0:v]trim=start={events[17]-1.7}:end={events[18]+.6},setpts=PTS-STARTPTS[b];[a][b]concat=n=2:v=1:a=0[v]'
    subprocess.run(['ffmpeg','-v','error','-y','-i',str(args.video),'-filter_complex',cuts,'-map','[v]','-an','-c:v','libx264','-crf','18',str(short)],check=True)
    encode(short,'settings-editor')
