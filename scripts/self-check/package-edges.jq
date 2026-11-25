# Input: relations.jsonl. Output: one line per reference or implementation between two
# workspace packages, "from<TAB>to<TAB>scope", where scope is "test" when the relation's first
# site lies in test code by its path, and "src" otherwise. $ws holds the workspace packages,
# separated by spaces.
($ws | split(" ")) as $packages
| select(.rel == "ref" or .rel == "implements")
| (.from | split(":")[1]) as $from
| (.to | split(":")[1]) as $to
| select($from != $to and ($packages | index([$from])) and ($packages | index([$to])))
| (.sites[0][0] | if test("(^|/)tests/|/tests\\.rs$|_test\\.go$") then "test" else "src" end) as $scope
| [$from, $to, $scope] | @tsv
