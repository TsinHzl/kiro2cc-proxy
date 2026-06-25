#!/usr/bin/env python3
"""扫描代理日志的 [usage] 入库 行,反推 sonnet/opus 的真实 k_ref。

公式（无缓存命中场景）：
    credits = k_ref * (input_price * input + output_price * output) / 1_000_000

若 cache_read = 0（无命中或反推判定为 0），将所有此类样本回归出
    credits / (input_price * input + output_price * output) ≈ k_ref / 1e6

把 cache_read > 0 的样本剔除（仅当 Kiro 真实透传）。
"""

import argparse
import re
import sys
from collections import defaultdict
from statistics import mean, median

# 同时匹配两种 [usage] 入库 格式：
#   stream.rs (流式): metering_credits=Some(x) ... cache_read=Some(N) cache_creation=Some(N)
#   handlers.rs (/cc/v1 缓冲): metering_credits=Some(x) ... cache_read=N cache_creation=N
USAGE_RE = re.compile(
    r"\[usage\] 入库: model=(?P<model>\S+) input=(?P<input>\d+) output=(?P<output>\d+) "
    r"metering_credits=Some\((?P<credits>[0-9.e+-]+)\).*?"
    r"cache_read=(?:Some\((?P<cr_s>\d+)\)|(?P<cr_b>\d+))\s+"
    r"cache_creation=(?:Some\((?P<cc_s>\d+)\)|(?P<cc_b>\d+))"
)

# 与 src/anthropic/stream.rs:601-610 对齐：opus/fable=(15,75)，haiku 不参与反推（k_ref 未知），其余按 sonnet。
PRICES = {
    "opus": (15.0, 75.0),
    "fable": (15.0, 75.0),
    "sonnet": (3.0, 15.0),
}
SKIP_FAMILIES = {"haiku"}  # 代码 stream.rs:606 对 haiku 返回 None

def model_family(name: str) -> str:
    if "haiku" in name:
        return "haiku"
    for k in ("opus", "fable", "sonnet"):
        if k in name:
            return k
    # 与代码 fallback 一致：未知模型按 sonnet 反推
    return "sonnet"

def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("logfile", help="代理日志路径")
    p.add_argument("--include-cached", action="store_true",
                   help="包含 cache_read>0 的样本（默认仅用 cache_read=0 样本反推基准 k_ref）")
    args = p.parse_args()

    by_model = defaultdict(list)
    by_model_cached = defaultdict(list)
    skipped = 0
    with open(args.logfile, encoding="utf-8", errors="ignore") as f:
        for line in f:
            m = USAGE_RE.search(line)
            if not m:
                continue
            model = m.group("model")
            fam = model_family(model)
            if fam in SKIP_FAMILIES:
                skipped += 1
                continue
            inp = int(m.group("input"))
            out = int(m.group("output"))
            credits = float(m.group("credits"))
            cr = int(m.group("cr_s") or m.group("cr_b"))
            cc = int(m.group("cc_s") or m.group("cc_b"))
            input_price, output_price = PRICES[fam]
            denom_usd = (input_price * inp + output_price * out) / 1_000_000.0
            if denom_usd <= 0:
                continue
            ratio = credits / denom_usd  # 估计 k_ref（假设无缓存）
            entry = (model, inp, out, credits, cr, cc, ratio)
            if cr == 0 and cc == 0:
                by_model[fam].append(entry)
            else:
                by_model_cached[fam].append(entry)

    print(f"# 校准 k_ref（当前代码硬编码 sonnet/opus k_ref = 1.43）\n")
    print("⚠️  注意: cache_read=0 来自代理 infer_cache_read_tokens 反推。")
    print("    若公式本身失效（如 k_ref 偏低），实际命中样本也会被误判为 cache_read=0,")
    print("    进而污染下面的『无缓存』基准回归。结果应与首次会话首条请求等已知")
    print("    『肯定无缓存』的样本对照。\n")
    if skipped:
        print(f"已跳过 haiku 等不参与反推的样本: {skipped}\n")

    for fam, rows in sorted(by_model.items()):
        ratios = [r[6] for r in rows]
        if not ratios:
            continue
        print(f"## {fam} (无缓存样本 n={len(ratios)})")
        print(f"  k_ref 估计: mean={mean(ratios):.4f}  median={median(ratios):.4f}  "
              f"min={min(ratios):.4f}  max={max(ratios):.4f}")
        print(f"  与硬编码 1.43 偏差: median 相对差 {(median(ratios) - 1.43) / 1.43 * 100:+.1f}%")
        print()

    if args.include_cached:
        print("# 含缓存命中样本（参考用,不可直接拟合 k_ref）\n")
        for fam, rows in sorted(by_model_cached.items()):
            ratios = [r[6] for r in rows]
            if not ratios:
                continue
            print(f"## {fam} (cache_read>0 n={len(ratios)})")
            print(f"  视为无缓存的 ratio: mean={mean(ratios):.4f}  "
                  f"median={median(ratios):.4f}")
            print()

    return 0

if __name__ == "__main__":
    sys.exit(main())
