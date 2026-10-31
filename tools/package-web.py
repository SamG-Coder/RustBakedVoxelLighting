"""Lossless static-site delivery: browsers decompress .bin.gz using DecompressionStream."""
from pathlib import Path
import gzip, shutil, hashlib, zipfile, argparse
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--archive',action='store_true')
args=parser.parse_args()
assets=root/'web/assets'
for path in assets.glob('*.bin'):
    target=path.with_suffix('.bin.gz')
    if not target.exists() or target.stat().st_mtime<path.stat().st_mtime:
        with path.open('rb') as source, gzip.GzipFile(filename=str(target),mode='wb',compresslevel=6,mtime=0) as output:
            shutil.copyfileobj(source,output)
if args.archive:
    archive=root/'dist/browser-assets-v1.zip'
    archive.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_STORED) as output:
        for path in sorted(assets.iterdir()):
            if path.name=='package.json' or path.suffix=='.gz':output.write(path, 'assets/'+path.name)
    digest=hashlib.file_digest(archive.open('rb'),'sha256').hexdigest()
    print(f'{digest}  {archive.name}')
    print(f'{archive.stat().st_size:,} bytes')
