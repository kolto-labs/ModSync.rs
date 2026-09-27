import json,sys,os,subprocess
tags=sys.argv[1:]
for k in sorted(os.listdir("cases")):
    d="cases/"+k
    outs={}
    for t in ["cs"]+tags:
        p=f"{d}/{t}.json"
        outs[t]=subprocess.run(["python3","summ.py",p],capture_output=True,text=True).stdout if os.path.exists(p) else "(none)\n"
    bad=lambda o: o.startswith("(none)") or o.startswith("ERR")
    verdict=" ".join(f"{t}={'SAME' if outs[t]==outs['cs'] and not bad(outs['cs']) else 'DIFF'}" for t in tags)
    print(f"== {k} [{open(d+'/flags').read()}] {verdict}")
    for t in ["cs"]+tags:
        if t=="cs" or outs[t]!=outs["cs"]:
            print(f" [{t}]"); print(outs[t],end="")
