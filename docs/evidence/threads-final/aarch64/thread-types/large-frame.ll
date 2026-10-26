; ModuleID = 'thread_large_frame.5b8630defa9eaa81-cgu.0'
source_filename = "thread_large_frame.5b8630defa9eaa81-cgu.0"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "aarch64-unknown-none"

; thread_large_frame::large_frame
; Function Attrs: noinline noredzone nounwind uwtable
define dso_local noundef i8 @_RNvCs7Rbe4ln1CJV_18thread_large_frame11large_frame(i64 noundef %index) unnamed_addr #0 !guid !2 {
start:
  %_3 = alloca [8 x i8], align 8
  %bytes = alloca [131072 x i8], align 1
  call void @llvm.lifetime.start.p0(ptr nonnull %bytes)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 1 dereferenceable(131072) %bytes, i8 0, i64 131072, i1 false)
  call void @llvm.lifetime.start.p0(ptr nonnull %_3)
  store ptr %bytes, ptr %_3, align 8
  call void asm sideeffect "", "r,~{memory}"(ptr nonnull %_3) #3, !srcloc !3
  call void @llvm.lifetime.end.p0(ptr nonnull %_3)
  %_5 = and i64 %index, 131071
  %0 = getelementptr inbounds nuw i8, ptr %bytes, i64 %_5
  %_0 = load i8, ptr %0, align 1, !noundef !4
  call void @llvm.lifetime.end.p0(ptr nonnull %bytes)
  ret i8 %_0
}

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite)
declare void @llvm.lifetime.start.p0(ptr captures(none)) #1

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: write)
declare void @llvm.memset.p0.i64(ptr writeonly captures(none), i8, i64, i1 immarg) #2

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite)
declare void @llvm.lifetime.end.p0(ptr captures(none)) #1

attributes #0 = { noinline noredzone nounwind uwtable "probe-stack"="inline-asm" "target-cpu"="generic" "target-features"="+v8a,+strict-align,+neon,+fp-armv8" }
attributes #1 = { mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: readwrite) }
attributes #2 = { mustprogress nocallback nofree nosync nounwind willreturn memory(argmem: write) }
attributes #3 = { nounwind }

!llvm.module.flags = !{!0}
!llvm.ident = !{!1}

!0 = !{i32 7, !"uwtable", i32 2}
!1 = !{!"rustc version 1.99.0 (b940084d7 2026-09-28)"}
!2 = !{i64 3427402145039724471}
!3 = !{i64 1651131457854863}
!4 = !{}
