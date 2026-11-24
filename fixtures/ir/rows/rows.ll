; ModuleID = 'rows.e49e8bfc74d1a7ca-cgu.0'
source_filename = "rows.e49e8bfc74d1a7ca-cgu.0"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

; rows::leaf
; Function Attrs: nonlazybind uwtable
define i32 @_RNvCsjCVZgSmFKGc_4rows4leaf() unnamed_addr #0 !dbg !8 !guid !15 {
start:
  ret i32 7, !dbg !16
}

; rows::caller
; Function Attrs: nonlazybind uwtable
define i32 @_RNvCsjCVZgSmFKGc_4rows6caller() unnamed_addr #0 !dbg !17 !guid !18 {
start:
; call rows::leaf
  %_0 = call i32 @_RNvCsjCVZgSmFKGc_4rows4leaf(), !dbg !19
  ret i32 %_0, !dbg !20
}

attributes #0 = { nonlazybind uwtable "probe-stack"="inline-asm" "target-cpu"="x86-64" }

!llvm.module.flags = !{!0, !1, !2, !3, !4}
!llvm.ident = !{!5}
!llvm.dbg.cu = !{!6}

!0 = !{i32 8, !"PIC Level", i32 2}
!1 = !{i32 2, !"RtLibUseGOT", i32 1}
!2 = !{i32 7, !"uwtable", i32 2}
!3 = !{i32 7, !"Dwarf Version", i32 4}
!4 = !{i32 2, !"Debug Info Version", i32 3}
!5 = !{!"rustc version 1.99.1 (58cf8f9c3 2026-10-13)"}
!6 = distinct !DICompileUnit(language: DW_LANG_Rust, file: !7, producer: "clang LLVM (rustc version 1.99.1 (58cf8f9c3 2026-10-13))", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug, splitDebugInlining: false, nameTableKind: None)
!7 = !DIFile(filename: "src/lib.rs/@/rows.e49e8bfc74d1a7ca-cgu.0", directory: "@ROOT@")
!8 = distinct !DISubprogram(name: "leaf", linkageName: "_RNvCsjCVZgSmFKGc_4rows4leaf", scope: !10, file: !9, line: 1, type: !11, scopeLine: 1, flags: DIFlagPrototyped, spFlags: DISPFlagDefinition, unit: !6, templateParams: !14)
!9 = !DIFile(filename: "src/lib.rs", directory: "@ROOT@", checksumkind: CSK_MD5, checksum: "34cb2f2a1489a93bacc40e216e394fb0")
!10 = !DINamespace(name: "rows", scope: null)
!11 = !DISubroutineType(types: !12)
!12 = !{!13}
!13 = !DIBasicType(name: "u32", size: 32, encoding: DW_ATE_unsigned)
!14 = !{}
!15 = !{i64 5058255256388570349}
!16 = !DILocation(line: 1, column: 27, scope: !8)
!17 = distinct !DISubprogram(name: "caller", linkageName: "_RNvCsjCVZgSmFKGc_4rows6caller", scope: !10, file: !9, line: 2, type: !11, scopeLine: 2, flags: DIFlagPrototyped, spFlags: DISPFlagDefinition, unit: !6, templateParams: !14)
!18 = !{i64 2665998342798921268}
!19 = !DILocation(line: 3, column: 5, scope: !17)
!20 = !DILocation(line: 4, column: 2, scope: !17)
