# Input: relations.jsonl, with $entities slurped from entities.jsonl. Output: one line per
# reference from code in one module to a definition in another, "from<TAB>to". A module is the
# top-level file or directory under a crate's src/ or the directory of a Go file.
# Use the current Rust adapter path on both sides when comparing across the move.
def module_of:
  if . == null then null
  elif test("^(crates/[^/]+|adapters/rust)/src/") then
    capture("^(?<m>(crates/[^/]+|adapters/rust)/src/[^/.]+)").m
    | sub("^crates/adapter-rust/"; "adapters/rust/")
  else (split("/") | .[:-1] | join("/"))
  end;
($entities | map(select(.def != null) | {(.["key"]): .def.file}) | add) as $file
| select(.rel == "ref")
| (.sites[0][0] | module_of) as $from
| ($file[.to] | module_of) as $to
| select($from != null and $to != null and $from != $to)
| [$from, $to] | @tsv
