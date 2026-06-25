#!/usr/bin/env python3
"""Minotari Ledger Wallet - Unified Cross-Platform Installer.

Replaces per-OS/per-model shell scripts with one installer that:
  1. Auto-detects the connected Ledger model via USB HID
  2. Downloads the correct firmware from GitHub Releases
  3. Installs it - all in one step

Usage:
    python3 minotari_ledger_install.py [--tag v5.2.0] [--model nanosplus]

Requirements:
    pip install requests ledgerwallet ledgerblue hidapi
"""
from __future__ import annotations
import argparse,platform,subprocess,sys,tempfile,zipfile
from pathlib import Path
try: import requests
except ImportError: sys.exit("pip install requests ledgerwallet ledgerblue hidapi")

MODELS={"nanos":{"target_id":"0x31100004","asset":"nanos","display":"Nano S"},
        "nanosplus":{"target_id":"0x33100004","asset":"nanosplus","display":"Nano S Plus"},
        "nanox":{"target_id":"0x33000004","asset":"nanox","display":"Nano X"},
        "stax":{"target_id":"0x33200004","asset":"stax","display":"Stax"},
        "flex":{"target_id":"0x33300004","asset":"flex","display":"Flex"}}
HID_PID={0x0011:"nanos",0x4011:"nanosplus",0x0015:"nanosplus",
         0x5011:"nanox",0x0040:"nanox",0x6011:"stax",0x0060:"stax",
         0x7011:"flex",0x0070:"flex"}
VENDOR=0x2C97
REPO="tari-project/tari"
APP="MinoTari Wallet"

def detect()->str|None:
    try:
        import hid
        for d in hid.enumerate(VENDOR,0):
            m=HID_PID.get(d.get("product_id",0))
            if m: return m
    except ImportError: pass
    try:
        out=subprocess.check_output(["ledgerctl","get-target-id"],text=True,timeout=10).strip().lower()
        for k,v in MODELS.items():
            if v["target_id"].lower()==out: return k
    except Exception: pass
    return None

def asset_url(model:str,tag:str|None)->str:
    base=f"https://api.github.com/repos/{REPO}/releases"
    url=f"{base}/tags/{tag}" if tag else f"{base}/latest"
    r=requests.get(url,timeout=30); r.raise_for_status()
    pat=MODELS[model]["asset"]
    for a in r.json().get("assets",[]):
        n=a["name"].lower()
        if pat in n and n.endswith(".zip"): return a["browser_download_url"]
    raise RuntimeError(f"No asset for {model!r}")

def install(model:str,apdu:Path)->None:
    tid=MODELS[model]["target_id"]
    subprocess.run(["ledgerctl","delete",APP],capture_output=True)
    rc=subprocess.run([sys.executable,"-m","ledgerblue.runScript",
        "--targetId",tid,"--fileName",str(apdu),"--apdu","--scp"]).returncode
    if rc!=0: raise RuntimeError("ledgerblue failed")

def main()->None:
    ap=argparse.ArgumentParser(description=__doc__,formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("-t","--tag",help="Release tag (default: latest)")
    ap.add_argument("-m","--model",choices=list(MODELS),help="Override auto-detect")
    a=ap.parse_args()
    print(f"Minotari Ledger Installer  [{platform.system()} {platform.machine()}]\n")
    model=a.model
    if not model:
        print("Step 1: Detecting Ledger...")
        model=detect()
        if not model:
            print("  ERROR: No Ledger found. Check USB, PIN, Developer Mode.\n  Or pass --model nanosplus|nanox|flex|stax")
            sys.exit(1)
    print(f"  Model: {MODELS[model]['display']}\n")
    print(f"Step 2: Fetching release (tag={a.tag or 'latest'})...")
    url=asset_url(model,a.tag); print(f"  {url}")
    with tempfile.TemporaryDirectory() as tmp:
        zp=Path(tmp)/"fw.zip"
        with requests.get(url,stream=True,timeout=120) as r:
            r.raise_for_status()
            with open(zp,"wb") as f:
                for chunk in r.iter_content(8192): f.write(chunk)
        print(f"  Downloaded {zp.stat().st_size//1024} KB")
        with zipfile.ZipFile(zp) as z: z.extractall(tmp)
        hits=list(Path(tmp).rglob("*.apdu"))
        if not hits: sys.exit("  ERROR: no .apdu in archive")
        apdu=hits[0]; print(f"  Firmware: {apdu.name}\n")
        print("Step 3: Installing - device must be unlocked and on home screen...")
        install(model,apdu)
    print(f"\nDone! {MODELS[model]['display']} - MinoTari Wallet installed.")

if __name__=="__main__": main()
