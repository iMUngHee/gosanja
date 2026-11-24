; ModuleID = '6ioqopafrauawqaowcojr5zfo'
source_filename = "6ioqopafrauawqaowcojr5zfo"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@vtable.0 = private constant <{ [24 x i8], ptr, ptr, ptr }> <{ [24 x i8] c"\00\00\00\00\00\00\00\00\08\00\00\00\00\00\00\00\08\00\00\00\00\00\00\00", ptr @_RNSNvYNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0INtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_once6vtableCsjoC0h555x8z_9calls_app, ptr @_RNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0CsjoC0h555x8z_9calls_app, ptr @_RNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0CsjoC0h555x8z_9calls_app }>, align 8, !dbg !0, !guid !24
@__rustc_debug_gdb_scripts_section__ = linkonce_odr unnamed_addr constant [34 x i8] c"\01gdb_load_rust_pretty_printers.py\00", section ".debug_gdb_scripts", align 1, !guid !25

; std::rt::lang_start::<()>
; Function Attrs: nonlazybind uwtable
define hidden i64 @_RINvNtCs8ZKwes1zzDd_3std2rt10lang_startuECsjoC0h555x8z_9calls_app(ptr %main, i64 %argc, ptr %argv, i8 %sigpipe) unnamed_addr #0 !dbg !36 !guid !51 {
start:
  %sigpipe.dbg.spill = alloca [1 x i8], align 1
  %argv.dbg.spill = alloca [8 x i8], align 8
  %argc.dbg.spill = alloca [8 x i8], align 8
  %main.dbg.spill = alloca [8 x i8], align 8
  %_7 = alloca [8 x i8], align 8
  store ptr %main, ptr %main.dbg.spill, align 8
    #dbg_declare(ptr %main.dbg.spill, !45, !DIExpression(), !52)
  store i64 %argc, ptr %argc.dbg.spill, align 8
    #dbg_declare(ptr %argc.dbg.spill, !46, !DIExpression(), !53)
  store ptr %argv, ptr %argv.dbg.spill, align 8
    #dbg_declare(ptr %argv.dbg.spill, !47, !DIExpression(), !54)
  store i8 %sigpipe, ptr %sigpipe.dbg.spill, align 1
    #dbg_declare(ptr %sigpipe.dbg.spill, !48, !DIExpression(), !55)
  store ptr %main, ptr %_7, align 8, !dbg !56
; call std::rt::lang_start_internal
  %_0 = call i64 @_RNvNtCs8ZKwes1zzDd_3std2rt19lang_start_internal(ptr %_7, ptr align 8 @vtable.0, i64 %argc, ptr %argv, i8 %sigpipe), !dbg !57
  ret i64 %_0, !dbg !58
}

; std::sys::backtrace::__rust_begin_short_backtrace::<fn(), ()>
; Function Attrs: noinline nonlazybind uwtable
define internal void @_RINvNtNtCs8ZKwes1zzDd_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECsjoC0h555x8z_9calls_app(ptr %f) unnamed_addr #1 !dbg !59 !guid !71 {
start:
  %dummy.dbg.spill = alloca [0 x i8], align 1
  %f.dbg.spill = alloca [8 x i8], align 8
  %result.dbg.spill = alloca [0 x i8], align 1
  %_2 = alloca [0 x i8], align 1
    #dbg_declare(ptr %result.dbg.spill, !67, !DIExpression(), !72)
  store ptr %f, ptr %f.dbg.spill, align 8
    #dbg_declare(ptr %f.dbg.spill, !66, !DIExpression(), !73)
    #dbg_declare(ptr %dummy.dbg.spill, !74, !DIExpression(), !82)
; call <fn() as core::ops::function::FnOnce<()>>::call_once
  call void @_RNvYFEuINtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_onceCsjoC0h555x8z_9calls_app(ptr %f) #5, !dbg !84
  call void asm sideeffect "", "~{memory}"(), !dbg !85, !srcloc !86
  ret void, !dbg !87
}

; std::rt::lang_start::<()>::{closure#0}
; Function Attrs: inlinehint nonlazybind uwtable
define internal i32 @_RNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0CsjoC0h555x8z_9calls_app(ptr align 8 %_1) unnamed_addr #2 !dbg !88 !guid !95 {
start:
  %self.dbg.spill = alloca [1 x i8], align 1
  %_1.dbg.spill = alloca [8 x i8], align 8
  store ptr %_1, ptr %_1.dbg.spill, align 8
    #dbg_declare(ptr %_1.dbg.spill, !94, !DIExpression(DW_OP_deref), !96)
  %_4 = load ptr, ptr %_1, align 8, !dbg !97
; call std::sys::backtrace::__rust_begin_short_backtrace::<fn(), ()>
  call void @_RINvNtNtCs8ZKwes1zzDd_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECsjoC0h555x8z_9calls_app(ptr %_4) #6, !dbg !98
; call <() as std::process::Termination>::report
  %self = call i8 @_RNvXsU_NtCs8ZKwes1zzDd_3std7processuNtB5_11Termination6reportCsjoC0h555x8z_9calls_app() #5, !dbg !99
  store i8 %self, ptr %self.dbg.spill, align 1, !dbg !99
    #dbg_declare(ptr %self.dbg.spill, !100, !DIExpression(), !117)
  %_0 = zext i8 %self to i32, !dbg !119
  ret i32 %_0, !dbg !127
}

; <std::rt::lang_start<()>::{closure#0} as core::ops::function::FnOnce<()>>::call_once::{shim:vtable#0}
; Function Attrs: inlinehint nonlazybind uwtable
define internal i32 @_RNSNvYNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0INtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_once6vtableCsjoC0h555x8z_9calls_app(ptr %_1) unnamed_addr #2 !dbg !128 !guid !142 {
start:
  %_1.dbg.spill = alloca [8 x i8], align 8
  %_2 = alloca [0 x i8], align 1
  store ptr %_1, ptr %_1.dbg.spill, align 8
    #dbg_declare(ptr %_1.dbg.spill, !137, !DIExpression(), !143)
    #dbg_declare(ptr %_2, !138, !DIExpression(), !143)
  %0 = load ptr, ptr %_1, align 8, !dbg !143
; call <std::rt::lang_start<()>::{closure#0} as core::ops::function::FnOnce<()>>::call_once
  %_0 = call i32 @_RNvYNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0INtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_onceCsjoC0h555x8z_9calls_app(ptr %0) #5, !dbg !143
  ret i32 %_0, !dbg !143
}

; calls_app::main
; Function Attrs: nonlazybind uwtable
define hidden void @_RNvCsjoC0h555x8z_9calls_app4main() unnamed_addr #0 !dbg !144 !guid !151 {
start:
  %total.dbg.spill = alloca [4 x i8], align 4, !dbg !152
; call calls_lib::direct
  %total = call i32 @_RNvCshPFz110IhNm_9calls_lib6direct(), !dbg !152
  store i32 %total, ptr %total.dbg.spill, align 4, !dbg !152
    #dbg_declare(ptr %total.dbg.spill, !148, !DIExpression(), !153)
  %_2 = call i32 @shared_entry() #7, !dbg !154
  ret void, !dbg !155
}

; <() as std::process::Termination>::report
; Function Attrs: inlinehint nonlazybind uwtable
define internal i8 @_RNvXsU_NtCs8ZKwes1zzDd_3std7processuNtB5_11Termination6reportCsjoC0h555x8z_9calls_app() unnamed_addr #2 !dbg !156 !guid !162 {
start:
  %_1.dbg.spill = alloca [0 x i8], align 1
    #dbg_declare(ptr %_1.dbg.spill, !161, !DIExpression(), !163)
  ret i8 0, !dbg !164
}

; <fn() as core::ops::function::FnOnce<()>>::call_once
; Function Attrs: inlinehint nonlazybind uwtable
define internal void @_RNvYFEuINtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_onceCsjoC0h555x8z_9calls_app(ptr %_1) unnamed_addr #2 !dbg !165 !guid !171 {
start:
  %_1.dbg.spill = alloca [8 x i8], align 8
  %_2 = alloca [0 x i8], align 1
  store ptr %_1, ptr %_1.dbg.spill, align 8
    #dbg_declare(ptr %_1.dbg.spill, !167, !DIExpression(), !172)
    #dbg_declare(ptr %_2, !168, !DIExpression(), !172)
  call void %_1(), !dbg !172
  ret void, !dbg !172
}

; <std::rt::lang_start<()>::{closure#0} as core::ops::function::FnOnce<()>>::call_once
; Function Attrs: inlinehint nonlazybind uwtable
define internal i32 @_RNvYNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0INtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_onceCsjoC0h555x8z_9calls_app(ptr %0) unnamed_addr #2 personality ptr @rust_eh_personality !dbg !173 !guid !179 {
start:
  %_2 = alloca [0 x i8], align 1
  %_1 = alloca [8 x i8], align 8
  store ptr %0, ptr %_1, align 8
    #dbg_declare(ptr %_1, !177, !DIExpression(), !180)
    #dbg_declare(ptr %_2, !178, !DIExpression(), !180)
; call std::rt::lang_start::<()>::{closure#0}
  %_0 = call i32 @_RNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0CsjoC0h555x8z_9calls_app(ptr align 8 %_1) #5, !dbg !180
  br label %bb1, !dbg !180

bb1:                                              ; preds = %start
  ret i32 %_0, !dbg !180

bb3:                                              ; No predecessors!
  unreachable

bb4:                                              ; No predecessors!
  unreachable
}

; std::rt::lang_start_internal
; Function Attrs: nonlazybind uwtable
declare i64 @_RNvNtCs8ZKwes1zzDd_3std2rt19lang_start_internal(ptr, ptr align 8, i64, ptr, i8) unnamed_addr #0

; calls_lib::direct
; Function Attrs: nonlazybind uwtable
declare i32 @_RNvCshPFz110IhNm_9calls_lib6direct() unnamed_addr #0

; Function Attrs: nounwind nonlazybind uwtable
declare i32 @shared_entry() unnamed_addr #3

; Function Attrs: nounwind nonlazybind uwtable
declare i32 @rust_eh_personality(i32, i32, i64, ptr, ptr) unnamed_addr #3

; Function Attrs: nonlazybind
define i32 @main(i32 %0, ptr %1) unnamed_addr #4 !guid !181 {
top:
  %2 = load volatile i8, ptr @__rustc_debug_gdb_scripts_section__, align 1
  %3 = sext i32 %0 to i64
; call std::rt::lang_start::<()>
  %4 = call i64 @_RINvNtCs8ZKwes1zzDd_3std2rt10lang_startuECsjoC0h555x8z_9calls_app(ptr @_RNvCsjoC0h555x8z_9calls_app4main, i64 %3, ptr %1, i8 0)
  %5 = trunc i64 %4 to i32
  ret i32 %5
}

attributes #0 = { nonlazybind uwtable "probe-stack"="inline-asm" "target-cpu"="x86-64" }
attributes #1 = { noinline nonlazybind uwtable "probe-stack"="inline-asm" "target-cpu"="x86-64" }
attributes #2 = { inlinehint nonlazybind uwtable "probe-stack"="inline-asm" "target-cpu"="x86-64" }
attributes #3 = { nounwind nonlazybind uwtable "probe-stack"="inline-asm" "target-cpu"="x86-64" }
attributes #4 = { nonlazybind "target-cpu"="x86-64" }
attributes #5 = { inlinehint }
attributes #6 = { noinline }
attributes #7 = { nounwind }

!llvm.module.flags = !{!26, !27, !28, !29, !30, !31}
!llvm.ident = !{!32}
!llvm.dbg.cu = !{!33}

!0 = !DIGlobalVariableExpression(var: !1, expr: !DIExpression())
!1 = distinct !DIGlobalVariable(name: "<std::rt::lang_start::{closure_env#0}<()> as core::ops::function::Fn<()>>::{vtable}", scope: null, file: !2, type: !3, isLocal: true, isDefinition: true)
!2 = !DIFile(filename: "<unknown>", directory: "")
!3 = !DICompositeType(tag: DW_TAG_structure_type, name: "<std::rt::lang_start::{closure_env#0}<()> as core::ops::function::Fn<()>>::{vtable_type}", file: !2, size: 384, align: 64, flags: DIFlagArtificial, elements: !4, vtableHolder: !14, templateParams: !23, identifier: "a94e9a5cfff3b3f8e33dadedff203dcd")
!4 = !{!5, !8, !10, !11, !12, !13}
!5 = !DIDerivedType(tag: DW_TAG_member, name: "drop_in_place", scope: !3, file: !2, baseType: !6, size: 64, align: 64)
!6 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "*const ()", baseType: !7, size: 64, align: 64, dwarfAddressSpace: 0)
!7 = !DIBasicType(name: "()", encoding: DW_ATE_unsigned)
!8 = !DIDerivedType(tag: DW_TAG_member, name: "size", scope: !3, file: !2, baseType: !9, size: 64, align: 64, offset: 64)
!9 = !DIBasicType(name: "usize", size: 64, encoding: DW_ATE_unsigned)
!10 = !DIDerivedType(tag: DW_TAG_member, name: "align", scope: !3, file: !2, baseType: !9, size: 64, align: 64, offset: 128)
!11 = !DIDerivedType(tag: DW_TAG_member, name: "__method3", scope: !3, file: !2, baseType: !6, size: 64, align: 64, offset: 192)
!12 = !DIDerivedType(tag: DW_TAG_member, name: "__method4", scope: !3, file: !2, baseType: !6, size: 64, align: 64, offset: 256)
!13 = !DIDerivedType(tag: DW_TAG_member, name: "__method5", scope: !3, file: !2, baseType: !6, size: 64, align: 64, offset: 320)
!14 = !DICompositeType(tag: DW_TAG_structure_type, name: "{closure_env#0}<()>", scope: !15, file: !2, size: 64, align: 64, elements: !18, templateParams: !23, identifier: "42f60e0049fd0b1afea23b60a8bae1ea")
!15 = !DINamespace(name: "lang_start", scope: !16)
!16 = !DINamespace(name: "rt", scope: !17)
!17 = !DINamespace(name: "std", scope: null)
!18 = !{!19}
!19 = !DIDerivedType(tag: DW_TAG_member, name: "main", scope: !14, file: !2, baseType: !20, size: 64, align: 64)
!20 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "fn()", baseType: !21, size: 64, align: 64, dwarfAddressSpace: 0)
!21 = !DISubroutineType(types: !22)
!22 = !{null}
!23 = !{}
!24 = !{i64 -5883946487050105666}
!25 = !{i64 -6971401179101347155}
!26 = !{i32 8, !"PIC Level", i32 2}
!27 = !{i32 7, !"PIE Level", i32 2}
!28 = !{i32 2, !"RtLibUseGOT", i32 1}
!29 = !{i32 7, !"uwtable", i32 2}
!30 = !{i32 7, !"Dwarf Version", i32 4}
!31 = !{i32 2, !"Debug Info Version", i32 3}
!32 = !{!"rustc version 1.99.1 (58cf8f9c3 2026-10-13)"}
!33 = distinct !DICompileUnit(language: DW_LANG_Rust, file: !34, producer: "clang LLVM (rustc version 1.99.1 (58cf8f9c3 2026-10-13))", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug, globals: !35, splitDebugInlining: false, nameTableKind: None)
!34 = !DIFile(filename: "calls-app/src/main.rs/@/6ioqopafrauawqaowcojr5zfo", directory: "@ROOT@")
!35 = !{!0}
!36 = distinct !DISubprogram(name: "lang_start<()>", linkageName: "_RINvNtCs8ZKwes1zzDd_3std2rt10lang_startuECsjoC0h555x8z_9calls_app", scope: !16, file: !37, line: 199, type: !38, scopeLine: 199, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !49, retainedNodes: !44)
!37 = !DIFile(filename: "/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/std/src/rt.rs", directory: "", checksumkind: CSK_MD5, checksum: "1ca814dd8a8df910f905fe83b6bafc35")
!38 = !DISubroutineType(types: !39)
!39 = !{!40, !20, !40, !41, !43}
!40 = !DIBasicType(name: "isize", size: 64, encoding: DW_ATE_signed)
!41 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "*const *const u8", baseType: !42, size: 64, align: 64, dwarfAddressSpace: 0)
!42 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "*const u8", baseType: !43, size: 64, align: 64, dwarfAddressSpace: 0)
!43 = !DIBasicType(name: "u8", size: 8, encoding: DW_ATE_unsigned)
!44 = !{!45, !46, !47, !48}
!45 = !DILocalVariable(name: "main", arg: 1, scope: !36, file: !37, line: 200, type: !20)
!46 = !DILocalVariable(name: "argc", arg: 2, scope: !36, file: !37, line: 201, type: !40)
!47 = !DILocalVariable(name: "argv", arg: 3, scope: !36, file: !37, line: 202, type: !41)
!48 = !DILocalVariable(name: "sigpipe", arg: 4, scope: !36, file: !37, line: 203, type: !43)
!49 = !{!50}
!50 = !DITemplateTypeParameter(name: "T", type: !7)
!51 = !{i64 2738079212767829057}
!52 = !DILocation(line: 200, column: 5, scope: !36)
!53 = !DILocation(line: 201, column: 5, scope: !36)
!54 = !DILocation(line: 202, column: 5, scope: !36)
!55 = !DILocation(line: 203, column: 5, scope: !36)
!56 = !DILocation(line: 206, column: 10, scope: !36)
!57 = !DILocation(line: 205, column: 5, scope: !36)
!58 = !DILocation(line: 211, column: 2, scope: !36)
!59 = distinct !DISubprogram(name: "__rust_begin_short_backtrace<fn(), ()>", linkageName: "_RINvNtNtCs8ZKwes1zzDd_3std3sys9backtrace28___rust_begin_short_backtraceFEuuECsjoC0h555x8z_9calls_app", scope: !61, file: !60, line: 162, type: !63, scopeLine: 162, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !69, retainedNodes: !65)
!60 = !DIFile(filename: "/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/std/src/sys/backtrace.rs", directory: "", checksumkind: CSK_MD5, checksum: "0469076862be40bd9e65965440a24fae")
!61 = !DINamespace(name: "backtrace", scope: !62)
!62 = !DINamespace(name: "sys", scope: !17)
!63 = !DISubroutineType(types: !64)
!64 = !{null, !20}
!65 = !{!66, !67}
!66 = !DILocalVariable(name: "f", arg: 1, scope: !59, file: !60, line: 162, type: !20)
!67 = !DILocalVariable(name: "result", scope: !68, file: !60, line: 166, type: !7, align: 8)
!68 = distinct !DILexicalBlock(scope: !59, file: !60, line: 166, column: 5)
!69 = !{!70, !50}
!70 = !DITemplateTypeParameter(name: "F", type: !20)
!71 = !{i64 6539193998921440900}
!72 = !DILocation(line: 166, column: 9, scope: !68)
!73 = !DILocation(line: 162, column: 43, scope: !59)
!74 = !DILocalVariable(name: "dummy", scope: !75, file: !76, line: 490, type: !7, align: 8)
!75 = distinct !DISubprogram(name: "black_box<()>", linkageName: "_RINvNtCskADBYZwJmSU_4core4hint9black_boxuECsjoC0h555x8z_9calls_app", scope: !77, file: !76, line: 490, type: !79, scopeLine: 490, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !49, retainedNodes: !81)
!76 = !DIFile(filename: "/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/hint.rs", directory: "", checksumkind: CSK_MD5, checksum: "720ecb12dbf1a304509abd161627e0e2")
!77 = !DINamespace(name: "hint", scope: !78)
!78 = !DINamespace(name: "core", scope: null)
!79 = !DISubroutineType(types: !80)
!80 = !{null, !7}
!81 = !{!74}
!82 = !DILocation(line: 490, column: 27, scope: !75, inlinedAt: !83)
!83 = !DILocation(line: 169, column: 5, scope: !68)
!84 = !DILocation(line: 166, column: 18, scope: !59)
!85 = !DILocation(line: 491, column: 5, scope: !75, inlinedAt: !83)
!86 = !{i64 2754976707889536}
!87 = !DILocation(line: 172, column: 2, scope: !59)
!88 = distinct !DISubprogram(name: "{closure#0}<()>", linkageName: "_RNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0CsjoC0h555x8z_9calls_app", scope: !15, file: !37, line: 206, type: !89, scopeLine: 206, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !49, retainedNodes: !93)
!89 = !DISubroutineType(types: !90)
!90 = !{!91, !92}
!91 = !DIBasicType(name: "i32", size: 32, encoding: DW_ATE_signed)
!92 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "&std::rt::lang_start::{closure_env#0}<()>", baseType: !14, size: 64, align: 64, dwarfAddressSpace: 0)
!93 = !{!94}
!94 = !DILocalVariable(name: "main", scope: !88, file: !37, line: 200, type: !20, align: 64)
!95 = !{i64 -5514133407028471626}
!96 = !DILocation(line: 200, column: 5, scope: !88)
!97 = !DILocation(line: 206, column: 70, scope: !88)
!98 = !DILocation(line: 206, column: 18, scope: !88)
!99 = !DILocation(line: 206, column: 76, scope: !88)
!100 = !DILocalVariable(name: "self", arg: 1, scope: !101, file: !102, line: 2289, type: !103)
!101 = distinct !DISubprogram(name: "to_i32", linkageName: "_RNvMsO_NtCs8ZKwes1zzDd_3std7processNtB5_8ExitCode6to_i32CsjoC0h555x8z_9calls_app", scope: !103, file: !102, line: 2289, type: !113, scopeLine: 2289, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !23, declaration: !115, retainedNodes: !116)
!102 = !DIFile(filename: "/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/std/src/process.rs", directory: "", checksumkind: CSK_MD5, checksum: "d0cb31a0cc6fab21e82717ff08551f66")
!103 = !DICompositeType(tag: DW_TAG_structure_type, name: "ExitCode", scope: !104, file: !2, size: 8, align: 8, flags: DIFlagPublic, elements: !105, templateParams: !23, identifier: "d506a7d0fa6bf3819d252f29774e2507")
!104 = !DINamespace(name: "process", scope: !17)
!105 = !{!106}
!106 = !DIDerivedType(tag: DW_TAG_member, name: "__0", scope: !103, file: !2, baseType: !107, size: 8, align: 8, flags: DIFlagPrivate)
!107 = !DICompositeType(tag: DW_TAG_structure_type, name: "ExitCode", scope: !108, file: !2, size: 8, align: 8, flags: DIFlagPublic, elements: !111, templateParams: !23, identifier: "3775964157544797e3ed9c9320a79b46")
!108 = !DINamespace(name: "common", scope: !109)
!109 = !DINamespace(name: "unix", scope: !110)
!110 = !DINamespace(name: "process", scope: !62)
!111 = !{!112}
!112 = !DIDerivedType(tag: DW_TAG_member, name: "__0", scope: !107, file: !2, baseType: !43, size: 8, align: 8, flags: DIFlagPrivate)
!113 = !DISubroutineType(types: !114)
!114 = !{!91, !103}
!115 = !DISubprogram(name: "to_i32", linkageName: "_RNvMsO_NtCs8ZKwes1zzDd_3std7processNtB5_8ExitCode6to_i32CsjoC0h555x8z_9calls_app", scope: !103, file: !102, line: 2289, type: !113, scopeLine: 2289, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit, templateParams: !23)
!116 = !{!100}
!117 = !DILocation(line: 2289, column: 19, scope: !101, inlinedAt: !118)
!118 = !DILocation(line: 206, column: 85, scope: !88)
!119 = !DILocation(line: 595, column: 9, scope: !120, inlinedAt: !126)
!120 = distinct !DISubprogram(name: "as_i32", linkageName: "_RNvMs8_NtNtNtNtCs8ZKwes1zzDd_3std3sys7process4unix6commonNtB5_8ExitCode6as_i32CsjoC0h555x8z_9calls_app", scope: !107, file: !121, line: 594, type: !122, scopeLine: 594, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !23, declaration: !125)
!121 = !DIFile(filename: "/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/std/src/sys/process/unix/common.rs", directory: "", checksumkind: CSK_MD5, checksum: "53af668c029e945f241d26a4682824a9")
!122 = !DISubroutineType(types: !123)
!123 = !{!91, !124}
!124 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "&std::sys::process::unix::common::ExitCode", baseType: !107, size: 64, align: 64, dwarfAddressSpace: 0)
!125 = !DISubprogram(name: "as_i32", linkageName: "_RNvMs8_NtNtNtNtCs8ZKwes1zzDd_3std3sys7process4unix6commonNtB5_8ExitCode6as_i32CsjoC0h555x8z_9calls_app", scope: !107, file: !121, line: 594, type: !122, scopeLine: 594, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit, templateParams: !23)
!126 = !DILocation(line: 2290, column: 16, scope: !101, inlinedAt: !118)
!127 = !DILocation(line: 206, column: 93, scope: !88)
!128 = distinct !DISubprogram(name: "call_once<std::rt::lang_start::{closure_env#0}<()>, ()>", linkageName: "_RNSNvYNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0INtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_once6vtableCsjoC0h555x8z_9calls_app", scope: !130, file: !129, line: 250, type: !133, scopeLine: 250, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !139, retainedNodes: !136)
!129 = !DIFile(filename: "/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/ops/function.rs", directory: "", checksumkind: CSK_MD5, checksum: "ae01f833f82cd27aa916c99d95502941")
!130 = !DINamespace(name: "FnOnce", scope: !131)
!131 = !DINamespace(name: "function", scope: !132)
!132 = !DINamespace(name: "ops", scope: !78)
!133 = !DISubroutineType(types: !134)
!134 = !{!91, !135}
!135 = !DIDerivedType(tag: DW_TAG_pointer_type, name: "*mut std::rt::lang_start::{closure_env#0}<()>", baseType: !14, size: 64, align: 64, dwarfAddressSpace: 0)
!136 = !{!137, !138}
!137 = !DILocalVariable(arg: 1, scope: !128, file: !129, line: 250, type: !135)
!138 = !DILocalVariable(arg: 2, scope: !128, file: !129, line: 250, type: !7)
!139 = !{!140, !141}
!140 = !DITemplateTypeParameter(name: "Self", type: !14)
!141 = !DITemplateTypeParameter(name: "Args", type: !7)
!142 = !{i64 2851156221119813474}
!143 = !DILocation(line: 250, column: 5, scope: !128)
!144 = distinct !DISubprogram(name: "main", linkageName: "_RNvCsjoC0h555x8z_9calls_app4main", scope: !146, file: !145, line: 5, type: !21, scopeLine: 5, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition | DISPFlagMainSubprogram, unit: !33, templateParams: !23, retainedNodes: !147)
!145 = !DIFile(filename: "calls-app/src/main.rs", directory: "@ROOT@", checksumkind: CSK_MD5, checksum: "18ed08bb993d9b582c24630b06af0c17")
!146 = !DINamespace(name: "calls_app", scope: null)
!147 = !{!148}
!148 = !DILocalVariable(name: "total", scope: !149, file: !145, line: 6, type: !150, align: 32)
!149 = distinct !DILexicalBlock(scope: !144, file: !145, line: 6, column: 5)
!150 = !DIBasicType(name: "u32", size: 32, encoding: DW_ATE_unsigned)
!151 = !{i64 1803440278851329955}
!152 = !DILocation(line: 6, column: 17, scope: !144)
!153 = !DILocation(line: 6, column: 9, scope: !149)
!154 = !DILocation(line: 7, column: 21, scope: !149)
!155 = !DILocation(line: 8, column: 2, scope: !144)
!156 = distinct !DISubprogram(name: "report", linkageName: "_RNvXsU_NtCs8ZKwes1zzDd_3std7processuNtB5_11Termination6reportCsjoC0h555x8z_9calls_app", scope: !157, file: !102, line: 2691, type: !158, scopeLine: 2691, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !23, retainedNodes: !160)
!157 = !DINamespace(name: "{impl#58}", scope: !104)
!158 = !DISubroutineType(types: !159)
!159 = !{!103, !7}
!160 = !{!161}
!161 = !DILocalVariable(arg: 1, scope: !156, file: !102, line: 2691, type: !7)
!162 = !{i64 -9068807948779961048}
!163 = !DILocation(line: 2691, column: 15, scope: !156)
!164 = !DILocation(line: 2693, column: 6, scope: !156)
!165 = distinct !DISubprogram(name: "call_once<fn(), ()>", linkageName: "_RNvYFEuINtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_onceCsjoC0h555x8z_9calls_app", scope: !130, file: !129, line: 250, type: !63, scopeLine: 250, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !169, retainedNodes: !166)
!166 = !{!167, !168}
!167 = !DILocalVariable(arg: 1, scope: !165, file: !129, line: 250, type: !20)
!168 = !DILocalVariable(arg: 2, scope: !165, file: !129, line: 250, type: !7)
!169 = !{!170, !141}
!170 = !DITemplateTypeParameter(name: "Self", type: !20)
!171 = !{i64 -1960012758844755201}
!172 = !DILocation(line: 250, column: 5, scope: !165)
!173 = distinct !DISubprogram(name: "call_once<std::rt::lang_start::{closure_env#0}<()>, ()>", linkageName: "_RNvYNCINvNtCs8ZKwes1zzDd_3std2rt10lang_startuE0INtNtNtCskADBYZwJmSU_4core3ops8function6FnOnceuE9call_onceCsjoC0h555x8z_9calls_app", scope: !130, file: !129, line: 250, type: !174, scopeLine: 250, flags: DIFlagPrototyped, spFlags: DISPFlagLocalToUnit | DISPFlagDefinition, unit: !33, templateParams: !139, retainedNodes: !176)
!174 = !DISubroutineType(types: !175)
!175 = !{!91, !14}
!176 = !{!177, !178}
!177 = !DILocalVariable(arg: 1, scope: !173, file: !129, line: 250, type: !14)
!178 = !DILocalVariable(arg: 2, scope: !173, file: !129, line: 250, type: !7)
!179 = !{i64 -2688037452074417116}
!180 = !DILocation(line: 250, column: 5, scope: !173)
!181 = !{i64 -2624081020897602054}
