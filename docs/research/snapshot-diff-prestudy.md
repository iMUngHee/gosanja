# Ranking possible moves between snapshots

On 2026-11-23 and 2026-11-24 we compared two pairs of public Rust releases. The question was whether a reader could find a moved definition among the additions using only the facts in the snapshots.

| Repository | Old tag and commit | New tag and commit |
| --- | --- | --- |
| https://github.com/dtolnay/itoa | 0.4.8, de247d6ac25d2e62d4cbd195f064ed4af35fd4eb | 1.0.0, e6a8f6f2f193aa852a3d2d84f2721e75d4517bff |
| https://github.com/marshallpierce/rust-base64 | v0.20.0, 9fcde482cd6aace0716c82b6472398d737c5b4d5 | v0.21.0, d7fb31c4ada4ca45df5ae80ec691fa3a050d9c3e |

## Inputs

We used gosanja 0.2.0 (566cc6b4430ef167399820f2b1151931fb84a649), schema 0.2, rust-analyzer 0.3.3049-standalone, and rustc 1.99.1. For each repository we checked out both tags in turn at the same absolute path. With the release checkout in ROOT, an external output directory in OUT, and the tag in TAG, the commands for each snapshot were:

```sh
git -C "$ROOT" checkout --detach "$TAG"
rust-analyzer scip "$ROOT" --output "$OUT/$TAG.scip" --num-threads 2
gosanja index --scip "$OUT/$TAG.scip" --root "$ROOT" --out "$OUT/$TAG"
gosanja validate "$OUT/$TAG" --root "$ROOT"
```

itoa had 52 and 57 entities, base64 531 and 567. Only base references were indexed. In itoa, 36 of 41 surviving entity keys changed coordinates. Every one of the 39 surviving reference edges changed its site evidence.

We froze truth.tsv at 20:39:16 UTC on 2026-11-23, after inspecting the source history and key inventories and before writing or running the matcher. Its SHA-256 is be21eb3729be71bcb2248ae5e32bed5ed06cf5b8e07e94ffb72751afeb460d25. For base64 we traced 20 moves or renames and three deletions using upstream commits 538e6d9, 402c57b, d00b4db, d0d2c14 and 64bbcc0. For itoa the list covers all 11 vanished keys: five deletions and six ambiguous benchmark continuities from a template consolidation.

## Local measurements

The exploratory matcher compared definitions of equal kind. It weighted name, file, ancestor names, body length, outgoing references and incoming references at 30, 18, 18, 12, 13 and 9 percent respectively. It used Python SequenceMatcher for sequences, counted reference sites, and selected disjoint pairs greedily at 0.48. A separate trial used token digests and a bottom-16 frequency sketch of tokens from verified tagged source files.

| base64 matcher | Listed positives correctly paired | Listed positives missed | Wrong proposals on listed items | Proposals outside the list (unjudged) |
| --- | ---: | ---: | ---: | ---: |
| Snapshot alone or with digest alone | 17 | 3 | 0 | 56 |
| With token sketch, with or without digest | 19 | 1 | 1 | 59 |

The snapshot matcher missed FastPortable → GeneralPurpose and FastPortable::from → GeneralPurpose::new after both name and file changed. The token sketch recovered these two. Every configuration missed FastPortableEstimate::from → GeneralPurposeEstimate::new, where the constructor changed a parameter, replaced a helper call with checked arithmetic and initialized another field. The sketch also paired the deleted EngineExtensions::decode_ez_vec helper with Engine::decode_slice. The two bodies looked similar enough to exceed its threshold despite the deletion in 64bbcc0. On itoa every configuration proposed five of the ambiguous continuities, all on the list, and matched none of the confirmed deletions. The remaining ambiguous row was a virtual module with no kind.

## Replay against the frozen base64 pair

The snapshot diff first pairs rows by symbol and containment evidence. A member of a changed file stays unpaired. Candidate scores use integer Levenshtein similarity for names and ancestor sequences, count each relation row once, and admit up to three alternatives per removal. A target can appear for several removals. A separate scratch replay estimated these results before implementation:

| Threshold | Listed pairs at rank 1 | Listed pairs absent | Wrong first candidates | Deletions with candidates |
| ---: | ---: | ---: | ---: | ---: |
| 400000 | 18 | 2 | 1 | 0 |
| 420000 | 18 | 2 | 0 | 0 |
| 440000 | 17 | 3 | 0 | 0 |

At 400000 the estimate constructor received an unrelated DecodeSliceError::from as its first candidate at 408000. At 440000 all three missed positives were the same type and two constructors missed by the exploratory snapshot matcher. All three had empty candidate lists. No listed positive appeared at rank two or three at these thresholds.

Leaving the five changed-file members unpaired increased removed and added entity counts from 98/134 to 103/139. Removed and added relation counts rose from 1024/1211 to 1093/1280, an increase of 69 on each side. None of the listed candidates, ranks or emitted scores changed.

The production reproduction uses the frozen snapshots and list under fixtures/diff-public and the integration test in crates/core/tests/diff_cases.rs. Its command is:

```sh
cargo test -p gosanja-core --test diff_cases
```

This was a purposive sample dominated by one documented rename family. The threshold was chosen on these inputs. The unjudged proposals, ambiguous benchmarks and absence of compiler-call data limit what the results say about other repositories and languages.
