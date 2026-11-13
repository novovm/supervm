.symfix <SUPERVM>\target\c-original-dmp-review-20261113\symbols
.sympath+ <SUPERVM>\target\c-crash-analysis-20261112T004637Z\pdb-analysis
.exepath <SUPERVM>\target\c-crash-analysis-20261112T004637Z\pdb-analysis;C:\Windows\System32
.reload
.echo ==== ANALYZE ====
!analyze -v
.echo ==== EXCEPTION ====
.exr -1
.echo ==== CONTEXT ====
.ecxr
r
.echo ==== STACK ====
kv
.echo ==== MODULES ====
lm t n
lmvm novovm_node
.echo ==== INSTRUCTION ====
u @rip-20 @rip+20
ln @rip
q
