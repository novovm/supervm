"""Reuse reviewed exception-only CDB launcher; no per-call breakpoints.
New binary and capture changes justify one timing qualification, not a replay of
the previously passed snapshot tests. Long reproduction is not authorized here.
"""
from pathlib import Path
HERE=Path(__file__).resolve().parent
old=HERE.parent/'C-lowimpact-capture-20261114/compare.py'
code=old.read_text(encoding='utf-8')
code=code.replace('target/c-lowimpact-capture-20261114','target/c-partitioned-writer-20261114')
code=code.replace("SERIES=ROOT/'heap-disabled'","SERIES=ROOT/'qualification'")
code=code.replace("'qualifier':'not original input replay; no business transfer; no relay restart'", "'qualifier':'partitioned recorder and copied std hooks qualification only; not original failure inputs; no deployed state'")
code=code.replace("reject for >=20% median query or poll P95 regression, >=20% loop throughput drop, missing target/probe evidence or capture errors; variance/incomplete context prevents root-cause inference", "reject for >=10% median-of-run query P50/P95/P99 or poll P95 regression, >=10% loop throughput drop, missing source Writer groups, AB loss or any target failure; small sample cannot establish scheduling equivalence")
code=code.replace("row={'elapsed':time.monotonic()-start,'nodes':[]}","row={'elapsed':time.monotonic()-start,'system_cpu_percent':psutil.cpu_percent(),'nodes':[]}")
exec(compile(code,str(old),'exec'),{'__name__':'__main__'})
