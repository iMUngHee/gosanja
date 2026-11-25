# Run with -n and $entities, $syntax and $relations slurped from a snapshot. Output: one line per
# definition outside test code that nothing else in the snapshot refers to, "file:line<TAB>kind
# <TAB>name<TAB>key", leaving out what is reachable from outside the snapshot: items written
# `pub` (an enum variant or a field by its parent's visibility), Go names that start with an
# upper-case letter, `main`, containers, and the members of a trait implementation. A collision
# member counts as referred to when its group is.
#
# These are candidates, not findings: a reference the indexer does not emit (one inside a
# `macro_rules!` body, or a name used only inside a format string) leaves a used item listed.
def in_test_path: test("(^|/)tests/|/tests\\.rs$|_test\\.go$");
($relations | map(select(.from != .to) | {(.to): true}) | add // {}) as $referred
| ($syntax | map({(.["key"]): (.attributes // [])}) | add // {}) as $attributes
| ($syntax | map({(.["key"]): (.visibility // "")}) | add // {}) as $visibility
| ($entities | map({(.["key"]): .parent}) | add // {}) as $parent
| def test_code($k):
    (($attributes[$k] // []) | any(test("^#\\[(test|bench)\\b|cfg\\(test\\)")))
    or ($parent[$k] as $p | $p != null and test_code($p));
  $entities[]
| select(.def != null)
| .["key"] as $k
| select(.def.file | in_test_path | not)
| select(.role != "container" and .name != "main")
| select($k | test("impl#\\[[^\\]]*\\]\\[") | not)
| (if (.kind // "") | test("enum-member$|field$") then $visibility[.parent] else $visibility[$k] end) as $v
| select(($v // "") | startswith("pub") | not)
| select(($k | startswith("gomod:") | not) or (.name | test("^[A-Z]") | not))
| select(($referred[$k] // false) == false and ((.member_of // null) as $g | $g == null or ($referred[$g] // false) == false))
| select(test_code($k) | not)
| ["\(.def.file):\(.def.name[0] + 1)", (.kind // ""), .name, $k] | @tsv
