import json, os
G=lambda n: f"{n:08d}-0000-0000-0000-000000000000"
def c(g,name,author="",**kw):
    d={"Guid":G(g),"Name":name,"Author":author,"IsSelected":True}
    d.update(kw); return d
def ins(action,src,dst="<<kotorDirectory>>\\Override",**kw):
    d={"Action":action,"Source":[src],"Destination":dst}; d.update(kw); return d
cases={}
# P1 relative insertion of unmatched existing
cases["p1_order"]=([c(1,"Alpha"),c(9,"Xeno Only"),c(2,"Beta")],[c(1,"Alpha"),c(2,"Beta"),c(3,"Gamma")],[])
# P2 GUID match must win over earlier name match
cases["p2_guid_first"]=([c(1,"Foo Mod","A"),c(2,"Bar Mod","B")],[c(2,"Foo Mod","A",Description="incoming")],[])
# P3 best heuristic score, not first hit
cases["p3_best_score"]=([c(11,"Big Texture Pack Lite","Ann"),c(12,"Big Texture Pack","Ann")],[c(21,"Big Texture Pack","Ann")],[])
# P4 domain fallback: jaccard 0.4 + same host 0.6
rr=lambda u:{u:{"Files":{"a.zip":True}}}
cases["p4_domain"]=([c(31,"Robes Upscaled HD Pack Extra","X",ResourceRegistry=rr("https://deadlystream.com/files/file/1-robes/"))],
                    [c(41,"Robes HD","Y",ResourceRegistry=rr("https://deadlystream.com/files/file/2-other/"))],[])
cases["p4b_domain_other_host"]=([c(31,"Robes Upscaled HD Pack Extra","X",ResourceRegistry=rr("https://deadlystream.com/files/file/1-robes/"))],
                    [c(41,"Robes HD","Y",ResourceRegistry=rr("https://www.nexusmods.com/kotor/mods/2"))],[])
# P5 punctuation: symbol vs unicode punctuation
cases["p5_symbol_plus"]=([c(51,"Mod+Plus")],[c(61,"ModPlus")],[])
cases["p5b_unicode_dash"]=([c(52,"Mod – Name")],[c(62,"Mod Name")],[])
# P6 exclusion flags
cases["p6_excl_incoming"]=([c(1,"Alpha"),c(9,"Old Only")],[c(1,"Alpha"),c(3,"New Only")],["--exclude-incoming-only"])
cases["p6b_excl_existing"]=([c(1,"Alpha"),c(9,"Old Only")],[c(1,"Alpha"),c(3,"New Only")],["--exclude-existing-only"])
cases["p6c_excl_incoming_existing_order"]=([c(1,"Alpha"),c(9,"Old Only")],[c(1,"Alpha"),c(3,"New Only")],["--exclude-incoming-only","--use-existing-order"])
cases["p6d_excl_existing_existing_order"]=([c(1,"Alpha"),c(9,"Old Only")],[c(1,"Alpha"),c(3,"New Only")],["--exclude-existing-only","--use-existing-order"])
# P7 language union + installation method
cases["p7_language_method"]=([c(1,"Alpha",Language=["English","German"],InstallationMethod="HoloPatcher")],[c(1,"Alpha",Language=["english","French"],InstallationMethod="Loose-File Mod")],[])
cases["p7b_method_prefer"]=([c(1,"Alpha",InstallationMethod="HoloPatcher")],[c(1,"Alpha",InstallationMethod="Loose-File Mod")],["--prefer-existing-installation-method"])
# P8 instruction merge: existing replaces on key collision
cases["p8_instr"]=([c(1,"Alpha",Instructions=[ins("Extract","<<modDirectory>>\\old.zip","<<modDirectory>>"),ins("Copy","<<modDirectory>>\\old\\*.tga")])],
                   [c(1,"Alpha",Instructions=[ins("Copy","<<modDirectory>>\\new\\*.tga"),ins("Delete","<<kotorDirectory>>\\x.tga")])],[])
# P9 duplicate instruction keys in target
cases["p9_dup_keys"]=([c(1,"Alpha",Instructions=[ins("Copy","<<modDirectory>>\\a.tga")])],
                      [c(1,"Alpha",Instructions=[ins("Copy","<<modDirectory>>\\b.tga"),ins("Copy","<<modDirectory>>\\c.tga")])],[])
# P10 use-existing-order + prefer-existing-name inversion; unmatched incoming insertion
cases["p10_existing_order"]=([c(1,"Old A"),c(2,"Old B")],[c(3,"New First"),c(2,"New B"),c(1,"New A")],["--use-existing-order","--prefer-existing-name"])
# P11 link validation: non-http target url dropped when source has links
cases["p11_links"]=([c(1,"Alpha",ResourceRegistry={"https://ok.example/a":{"Files":{"a.zip":True}},"ftp://bad.example/b":{"Files":{"b.zip":True}}})],
                    [c(1,"Alpha",ResourceRegistry={"https://ok.example/c":{"Files":{"c.zip":True}},"ftp://bad.example/d":{"Files":{"d.zip":True}}})],[])
# P12 heuristic-matched GUIDs differ: default keeps existing guid
cases["p12_guid_keep"]=([c(71,"Same Name","Auth")],[c(81,"Same Name","Auth")],[])
cases["p12b_guid_keep_existing_order"]=([c(71,"Same Name","Auth")],[c(81,"Same Name","Auth")],["--use-existing-order"])
# P13 prefer-existing-fields global
cases["p13_prefer_all"]=([c(1,"Old",Description="old d",Tier="1 - Essential")],[c(1,"New",Description="new d",Tier="2 - Recommended")],["--prefer-existing-fields"])
# P14 SkipBlank: blank existing does not overwrite
cases["p14_blank"]=([c(1,"Alpha",Description="   ")],[c(1,"Alpha",Description="keep me")],["--prefer-existing-description"])
for k,(e,i,flags) in cases.items():
    d=f"cases/{k}"; os.makedirs(d,exist_ok=True)
    json.dump(e,open(f"{d}/existing.json","w"),indent=1); json.dump(i,open(f"{d}/incoming.json","w"),indent=1)
    open(f"{d}/flags","w").write(" ".join(flags))
print(len(cases))
