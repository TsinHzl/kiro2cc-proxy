#!/usr/bin/env python3
from __future__ import annotations

import argparse
import colorsys
import contextlib
import io
import math
import os
import re
import sys
import tempfile
from typing import Dict, List, Optional, Tuple

RGBA = Tuple[float, float, float, float]
Tokens = Dict[str, str]

_HEX = re.compile(r"^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$")
_FUNC = re.compile(r"^(rgba?|hsla?)\(\s*(.+?)\s*\)$", re.I)
_TRIPLE = re.compile(r"^([\d.]+)(?:deg)?[\s,]+([\d.]+)%[\s,]+([\d.]+)%$")

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "src")
MIN_RATIO = 4.5
FALLBACK = {"brand-ink": "brand"}
_COMMENT = re.compile(r"/\*.*?\*/", re.S)
_VAR = re.compile(r"(--[a-z0-9-]+)\s*:\s*([^;]+);")


class ToolError(Exception):
    """工具自身故障(文件缺失、无法解析、无解)，退出码 3，区别于审计失败的 1。"""


def fmt_ratio(r: float) -> str:
    return f"{math.floor(r * 1000) / 1000:.3f}"

_SURFACES = ["surface", "surface-2", "surface-3", "sidebar"]
_SOFTS = ["brand-soft", "ok-soft", "warn-soft", "danger-soft"]
FULL_MATRIX = [
    (["foreground", "card-foreground", "popover-foreground"], ["background", "card", "popover"]),
    (["foreground"], ["surface-2", "surface-3", "sidebar", "muted"]),
    (["ink", "ink-2", "ink-3"], _SURFACES),
    (["ink", "ink-2", "ink-3"], ["background", "card", "muted"] + _SOFTS),
    (["muted-foreground"], ["background", "card", "muted"] + _SURFACES),
    (["secondary-foreground"], ["secondary"]),
    (["accent-foreground"], ["accent"]),
    (["brand-ink"], ["background", "surface", "surface-2", "surface-3", "sidebar", "brand-soft", "ok-soft"]),
    (["ok", "warn", "danger"], ["surface", "surface-2", "surface-3"] + _SOFTS),
    (["brand-fg"], ["brand", "brand-hover"]),
    (["primary-foreground"], ["primary"]),
    (["destructive-foreground"], ["destructive"]),
    (["quota-safe-badge-text"], ["quota-safe-badge-bg"]),
    (["quota-mod-badge-text"], ["quota-mod-badge-bg"]),
    (["quota-warn-badge-text"], ["quota-warn-badge-bg"]),
    (["quota-danger-badge-text"], ["quota-danger-badge-bg"]),
    (["badge-new-fg"], ["badge-new-from", "badge-new-to"]),
]
_LEGACY_PAIRS = [
    ("foreground", "background"), ("card-foreground", "card"), ("popover-foreground", "popover"),
    ("primary-foreground", "primary"), ("secondary-foreground", "secondary"),
    ("muted-foreground", "background"), ("muted-foreground", "card"), ("muted-foreground", "muted"),
    ("accent-foreground", "accent"), ("destructive-foreground", "destructive"),
    ("ink", "surface"), ("ink", "surface-3"), ("ink-2", "surface"), ("ink-2", "surface-2"),
    ("ink-2", "surface-3"), ("ink-2", "sidebar"), ("ink-3", "surface"), ("ink-3", "surface-3"),
    ("brand-fg", "brand"), ("brand-fg", "brand-hover"),
    ("brand", "surface"), ("brand", "sidebar"), ("brand", "brand-soft"),
    ("ok", "ok-soft"), ("warn", "warn-soft"), ("danger", "danger-soft"),
    ("ok", "surface"), ("warn", "surface"), ("danger", "surface"),
    ("quota-safe-badge-text", "quota-safe-badge-bg"), ("quota-mod-badge-text", "quota-mod-badge-bg"),
    ("quota-warn-badge-text", "quota-warn-badge-bg"), ("quota-danger-badge-text", "quota-danger-badge-bg"),
    ("badge-new-fg", "badge-new-from"), ("badge-new-fg", "badge-new-to"),
]
LEGACY_MATRIX = [([f], [b]) for f, b in _LEGACY_PAIRS]

FILL_TEXT = {"brand-fg", "primary-foreground", "destructive-foreground"}
FIX_ORDER = [
    "foreground", "card-foreground", "popover-foreground", "secondary-foreground",
    "accent-foreground", "muted-foreground", "primary-foreground", "destructive-foreground",
    "ink", "ink-3", "ink-2", "brand-fg", "ok", "warn", "danger",
    "quota-safe-badge-text", "quota-mod-badge-text", "quota-warn-badge-text",
    "quota-danger-badge-text", "badge-new-fg", "brand-ink",
]


def _q8(x: float) -> float:
    return float(math.floor(x + 0.5))


def hsl_to_rgb(h: float, s: float, l: float) -> Tuple[float, float, float]:
    h = (h % 360) / 360
    s, l = s / 100, l / 100
    if s == 0:
        v = _q8(l * 255)
        return v, v, v
    q = l * (1 + s) if l < 0.5 else l + s - l * s
    p = 2 * l - q

    def f(t: float) -> float:
        t %= 1
        if t < 1 / 6:
            return p + (q - p) * 6 * t
        if t < 1 / 2:
            return q
        if t < 2 / 3:
            return p + (q - p) * (2 / 3 - t) * 6
        return p

    return _q8(f(h + 1 / 3) * 255), _q8(f(h) * 255), _q8(f(h - 1 / 3) * 255)


def _chan(p: str) -> float:
    return float(p[:-1]) * 2.55 if p.endswith("%") else float(p)


def _alpha(p: str) -> float:
    return float(p[:-1]) / 100 if p.endswith("%") else float(p)


def parse_color(text: str) -> RGBA:
    """返回 (r, g, b, a)：通道 0-255 浮点，a 0-1。支持 hex、rgb(a)、hsl(a)、裸 HSL 三元组。"""
    s = text.strip().rstrip(";").strip()
    m = _HEX.match(s)
    if m:
        h = m.group(1)
        if len(h) in (3, 4):
            h = "".join(c * 2 for c in h)
        r, g, b = (float(int(h[i:i + 2], 16)) for i in (0, 2, 4))
        a = int(h[6:8], 16) / 255 if len(h) == 8 else 1.0
        return r, g, b, a
    m = _TRIPLE.match(s)
    if m:
        return (*hsl_to_rgb(*(float(x) for x in m.groups())), 1.0)
    m = _FUNC.match(s)
    if m:
        parts = re.split(r"[\s,/]+", m.group(2))
        a = _alpha(parts[3]) if len(parts) == 4 else 1.0
        if m.group(1).lower().startswith("rgb"):
            r, g, b = (_chan(p) for p in parts[:3])
        else:
            r, g, b = hsl_to_rgb(
                float(parts[0].replace("deg", "")),
                float(parts[1].rstrip("%")),
                float(parts[2].rstrip("%")),
            )
        return r, g, b, a
    raise ValueError(f"无法解析颜色: {text!r}")


def composite(top, base):
    r, g, b, a = top
    return (
        r * a + base[0] * (1 - a),
        g * a + base[1] * (1 - a),
        b * a + base[2] * (1 - a),
        1.0,
    )


def _lin(c):
    c /= 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def luminance(rgb):
    return 0.2126 * _lin(rgb[0]) + 0.7152 * _lin(rgb[1]) + 0.0722 * _lin(rgb[2])


def contrast(fg: RGBA, bg: RGBA) -> float:
    hi, lo = sorted((luminance(fg), luminance(bg)), reverse=True)
    return (hi + 0.05) / (lo + 0.05)


def _expect(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


_FIXTURE_TEXT = [n for n in FIX_ORDER if n != "brand-ink"]
_FIXTURE_FILLS = {"brand": "#2563eb", "brand-hover": "#1d4ed8", "primary": "#2563eb", "destructive": "#dc2626"}


def _fixture_tokens() -> Tokens:
    names = {n for fgs, bgs in FULL_MATRIX for n in fgs + bgs} - {"brand-ink"}
    toks = {n: "#ffffff" for n in names}
    toks.update({n: "#111111" for n in _FIXTURE_TEXT})
    toks["ink-3"] = "#444444"
    toks.update(_FIXTURE_FILLS)
    toks.update({"brand-fg": "#ffffff", "primary-foreground": "#ffffff", "destructive-foreground": "#ffffff"})
    return toks


def _selftest_fix() -> None:
    global SRC
    old_src = SRC
    quiet = io.StringIO()
    with tempfile.TemporaryDirectory() as tmp:
        SRC = tmp
        try:
            root = "\n".join(f"  --{k}: {v};" for k, v in _fixture_tokens().items())
            with open(os.path.join(tmp, "index.css"), "w", encoding="utf-8") as fh:
                fh.write(f":root {{\n{root}\n}}\n.dark {{\n  --background: #ffffff;\n}}\n")
            themes = os.path.join(tmp, "themes.css")
            with open(themes, "w", encoding="utf-8") as fh:
                fh.write(":root.theme-x {\n  --ink: #808080; /* a; b */\n  --ink-3: #909090;\n}\n:root.dark.theme-x {\n}\n")
            ns = argparse.Namespace(file=themes, out=None, suggest_base=False, base_only=False,
                                    matrix="full", verbose=False)
            with open(themes, encoding="utf-8") as fh:
                before = fh.read()
            with contextlib.redirect_stdout(quiet):
                _expect(run_check(ns) == 1, "fixture 修复前应审计失败")
                _expect(run_fix(ns) == 0, "第一次 --fix 应成功")
            with open(themes, encoding="utf-8") as fh:
                once = fh.read()
            with contextlib.redirect_stdout(quiet):
                _expect(run_fix(ns) == 0, "第二次 --fix 应成功")
            with open(themes, encoding="utf-8") as fh:
                twice = fh.read()
            _expect(once == twice, "--fix 不幂等")
            _expect(once != before and "/* a; b */" in once, "--fix 未改写或破坏了块内注释")
            orig = os.path.join(tmp, "orig.css")
            with open(orig, "w", encoding="utf-8") as fh:
                fh.write(before)
            with contextlib.redirect_stdout(quiet):
                _expect(run_check(ns) == 0, "修复后应审计通过")
                _expect(compare_fills(orig, themes) == 0, "--fix 改动了白名单之外的令牌")
            with open(themes, "w", encoding="utf-8") as fh:
                fh.write(":root.theme-x,\n:root.theme-y {\n}\n")
            try:
                load_variants(themes, False)
            except ToolError:
                pass
            else:
                _expect(False, "分组选择器应被识别并报错")
            try:
                write_block(":root.theme-x {\n}\n", ":root.dark.theme-x", {"ink": "#000000"})
            except ToolError:
                pass
            else:
                _expect(False, "缺失选择器块应报错")
        finally:
            SRC = old_src


def selftest() -> None:
    black, white = parse_color("#000"), parse_color("#fff")
    _expect(abs(contrast(black, white) - 21.0) < 1e-9, "黑白对比度应为 21")
    c = contrast(parse_color("#767676"), white)
    _expect(4.54 <= c <= 4.55, f"#767676 对白对比度异常: {c}")
    _expect(parse_color("hsl(0 0% 100%)") == parse_color("#ffffff"), "hsl() 白色解析错误")
    _expect(parse_color("0 0% 100%") == parse_color("#ffffff"), "HSL 三元组白色解析错误")
    half = composite(parse_color("rgba(0,0,0,.5)"), white)
    _expect(abs(luminance(half) - _lin(127.5)) < 1e-12, "半透明叠加错误")
    _expect(parse_color("rgba(10, 20, 30, 50%)")[3] == 0.5, "百分比 alpha 解析错误")
    _expect(fmt_ratio(4.4996) == "4.499", "比值显示不应向上取整到 4.50")
    _expect(_pure_black("0 0% 100%") == "0 0% 0%" and _pure_black("#fff") == "#000000", "纯黑回退应跟随令牌格式")
    _expect(all(c == int(c) for c in parse_color("hsl(200 50% 40%)")[:3]), "hsl 通道应量化为 8 位整数")
    _expect(next(candidates("rgba(10, 20, 30, 0.5)", 1)).startswith("rgba("), "alpha<1 的候选应保留 alpha")
    wb = write_block(
        ":root.theme-x {\n  --ink: #111; /* --ink: #222; */\n  --ink-3: #444\n}\n",
        ":root.theme-x",
        {"ink": "#000", "brand-ink": "#00f"},
    )
    _expect("--ink: #000; /* --ink: #222; */" in wb, "write_block 应改活动声明而不是注释")
    _expect("--ink-3: #444;\n    --brand-ink: #00f;" in wb, "write_block 应为缺分号的末项补分号")
    _selftest_fix()
    print("selftest OK")


def parse_blocks(path: str, selector_re: str) -> Dict[str, Tokens]:
    with open(path, encoding="utf-8") as fh:
        text = _COMMENT.sub("", fh.read())
    out: Dict[str, Tokens] = {}
    for m in re.finditer(r"(?m)^\s*(" + selector_re + r")\s*\{([^}]*)\}", text):
        out.setdefault(m.group(1), {}).update({k: v.strip() for k, v in _VAR.findall(m.group(2))})
    return out


_THEME_HEADER = re.compile(r"^\s*:root(?:\.dark)?\.theme-[a-z0-9-]+\s*\{\s*$")


def _check_theme_selectors(path: str) -> None:
    with open(path, encoding="utf-8") as fh:
        text = _COMMENT.sub(lambda mm: "\n" * mm.group().count("\n"), fh.read())
    depth = 0
    for no, line in enumerate(text.splitlines(), 1):
        if depth == 0 and ".theme-" in line and not _THEME_HEADER.match(line):
            raise ToolError(f"{path}:{no}: 无法识别的主题选择器(需独占一行 ':root[.dark].theme-<id> {{'): {line.strip()}")
        depth += line.count("{") - line.count("}")


def load_variants(themes_path: str, base_only: bool) -> List[Tuple[str, str, Tokens]]:
    base = parse_blocks(os.path.join(SRC, "index.css"), r":root|\.dark")
    root, dark = base.get(":root", {}), base.get(".dark", {})
    variants = [("base", "light", dict(root)), ("base", "dark", {**root, **dark})]
    if base_only:
        return variants
    _check_theme_selectors(themes_path)
    th = parse_blocks(themes_path, r":root(?:\.dark)?\.theme-[a-z0-9-]+")
    if not th:
        raise ToolError(f"{themes_path}: 未解析到任何主题块")
    for n in sorted({s.split(".theme-")[1] for s in th}):
        light = th.get(":root.theme-" + n, {})
        dk = th.get(":root.dark.theme-" + n, {})
        variants.append((n, "light", {**root, **light}))
        variants.append((n, "dark", {**root, **dark, **light, **dk}))
    return variants


def _color(tokens, name):
    key = "--" + name
    if key not in tokens and name in FALLBACK:
        key = "--" + FALLBACK[name]
    return parse_color(tokens[key])


def pair_ratio(tokens, fg, bg, base_name):
    b = _color(tokens, bg)
    if b[3] < 1:
        b = composite(b, _color(tokens, base_name))
    f = _color(tokens, fg)
    if f[3] < 1:
        f = composite(f, b)
    return contrast(f, b)


def check_variant(tokens, matrix, bases, hierarchy):
    fails, checks = [], 0
    for fgs, bgs in matrix:
        for fg in fgs:
            for bg in bgs:
                checks += 1
                try:
                    r = min(pair_ratio(tokens, fg, bg, b) for b in bases)
                except (KeyError, ValueError) as e:
                    fails.append((fg, bg, None, f"令牌缺失或无法解析: {e}"))
                    continue
                if r < MIN_RATIO:
                    fails.append((fg, bg, r, ""))
    if hierarchy:
        checks += 1
        try:
            gap = pair_ratio(tokens, "ink-2", "surface", "surface") - pair_ratio(tokens, "ink-3", "surface", "surface")
            if gap < 0.2:
                fails.append(("ink-2", "ink-3 层级(要求差 ≥ 0.2)", gap, ""))
        except (KeyError, ValueError) as e:
            fails.append(("ink-2", "ink-3 层级", None, f"令牌缺失或无法解析: {e}"))
    return fails, checks


def run_check(args):
    legacy = args.matrix == "legacy"
    matrix = LEGACY_MATRIX if legacy else FULL_MATRIX
    bases = ["surface"] if legacy else ["surface", "surface-2"]
    failing, total = [], 0
    variants = load_variants(args.file, args.base_only)
    for name, mode, tokens in variants:
        fails, n = check_variant(tokens, matrix, bases, not legacy)
        total += n
        failing += [(name, mode) + f for f in fails]
    if args.verbose:
        for name, mode, fg, bg, r, note in failing:
            print(f"{name}/{mode}: {fg} on {bg} = {note or fmt_ratio(r)}")
    by_variant, by_pair = {}, {}
    for name, mode, fg, bg, r, note in failing:
        by_variant[f"{name}/{mode}"] = by_variant.get(f"{name}/{mode}", 0) + 1
        by_pair.setdefault((fg, bg), []).append(r)
    for k, n in sorted(by_variant.items(), key=lambda x: -x[1])[:10]:
        print(f"变体 {k}: {n} 项")
    for (fg, bg), rs in sorted(by_pair.items(), key=lambda x: -len(x[1]))[:15]:
        known = [r for r in rs if r is not None]
        low = f"最低 {fmt_ratio(min(known))}" if known else "令牌缺失或无法解析"
        print(f"{fg} on {bg}: {len(rs)} 个变体, {low}")
    print(f"变体 {len(variants)} 个, 检查项 {total}, 失败项 {len(failing)}")
    return 1 if failing else 0


def bgs_of(fg):
    out = []
    for fgs, bgs in FULL_MATRIX:
        if fg in fgs:
            out += [b for b in bgs if b not in out]
    return out


def candidates(text, direction):
    m = _TRIPLE.match(text.strip())
    if m:
        h, s, l = m.group(1), m.group(2), float(m.group(3))
        for k in range(101):
            nl = l + direction * k
            if 0 <= nl <= 100:
                yield f"{h} {s}% {nl:g}%"
        return
    r, g, b, a = parse_color(text)
    h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
    for k in range(101):
        nl = l + direction * k / 100
        if 0 <= nl <= 1:
            rr, gg, bb = (round(c * 255) for c in colorsys.hls_to_rgb(h, nl, s))
            if a >= 1.0:
                yield "#%02x%02x%02x" % (rr, gg, bb)
            else:
                yield "rgba(%d, %d, %d, %g)" % (rr, gg, bb, a)


def _extremes(text):
    if _TRIPLE.match(text.strip()):
        return ["0 0% 100%", "210 12% 7%"]
    return ["#ffffff", "#0f1113"]


def _pure_black(text: str) -> str:
    return "0 0% 0%" if _TRIPLE.match(text.strip()) else "#000000"


def _hier(t):
    gap = pair_ratio(t, "ink-2", "surface", "surface") - pair_ratio(t, "ink-3", "surface", "surface")
    return gap >= 0.2


def solve(tokens, name, bgs, bases, extra=None):
    cur = tokens.get("--" + name) or (tokens["--brand"] if name == "brand-ink" else None)
    if cur is None:
        return None

    def ok(c):
        t = {**tokens, "--" + name: c}
        good = all(min(pair_ratio(t, name, bg, b) for b in bases) >= MIN_RATIO for bg in bgs)
        return good and (extra is None or extra(t))

    if ok(cur):
        return cur
    base_lum = luminance(parse_color(cur))
    if name in FILL_TEXT:
        pool = [c for d in (1, -1) for c in candidates(cur, d)] + _extremes(cur)
        good = [c for c in pool if ok(c)]
        return min(good, key=lambda c: abs(luminance(parse_color(c)) - base_lum)) if good else None
    surface = parse_color(tokens["--surface"])
    bg_lum = sum(luminance(composite(parse_color(tokens["--" + b]), surface)) for b in bgs) / len(bgs)
    pref = -1 if base_lum < bg_lum else 1
    for d in (pref, -pref):
        for c in candidates(cur, d):
            if ok(c):
                return c
    return None


def fix_brand_fg_with_hover(t: Tokens) -> Optional[Tuple[str, str]]:
    brand = parse_color(t["--brand"])
    for fg in (_extremes(t["--brand-fg"])[1], _pure_black(t["--brand-fg"])):
        if contrast(parse_color(fg), brand) < MIN_RATIO:
            continue
        for h in candidates(t["--brand-hover"], 1):
            if contrast(parse_color(fg), parse_color(h)) >= MIN_RATIO:
                return fg, h
    return None


def fix_badge_new_with_to(t: Tokens) -> Optional[Tuple[str, str]]:
    start = parse_color(t["--badge-new-from"])
    for fg in (_extremes(t["--badge-new-fg"])[1], _pure_black(t["--badge-new-fg"])):
        if contrast(parse_color(fg), start) < MIN_RATIO:
            continue
        for c in candidates(t["--badge-new-to"], 1):
            if contrast(parse_color(fg), parse_color(c)) >= MIN_RATIO:
                return fg, c
    return None


def fix_variant(tokens: Tokens, bases: List[str], label: str) -> Tokens:
    out, t = {}, dict(tokens)
    for name in FIX_ORDER:
        try:
            bgs = [b for b in bgs_of(name) if "--" + b in t]
            new = solve(t, name, bgs, bases, _hier if name == "ink-2" else None)
            if new is None and name == "brand-fg":
                res = fix_brand_fg_with_hover(t)
                if res:
                    new = res[0]
                    out["brand-hover"] = t["--brand-hover"] = res[1]
            if new is None and name == "badge-new-fg":
                res = fix_badge_new_with_to(t)
                if res:
                    new = res[0]
                    out["badge-new-to"] = t["--badge-new-to"] = res[1]
        except (KeyError, ValueError) as e:
            raise ToolError(f"无法处理 {label} --{name}: {e}") from e
        if new is None:
            raise ToolError(f"无解: {label} --{name}")
        if name == "brand-ink" or t.get("--" + name) != new:
            out[name] = new
        t["--" + name] = new
    return out


def write_block(text: str, selector: str, values: Tokens) -> str:
    if not values:
        return text
    m = re.search(r"(?m)^\s*" + re.escape(selector) + r"\s*\{", text)
    if m is None:
        raise ToolError(f"themes.css 缺少选择器块: {selector}")
    start = m.end()
    end = text.index("}", start)
    body = text[start:end]
    for name, val in values.items():
        pat = re.compile(r"(--" + re.escape(name) + r"\s*:\s*)([^;]+)(;)")
        masked = _COMMENT.sub(lambda mm: " " * len(mm.group()), body)
        hits = list(pat.finditer(masked))
        if hits:
            h = hits[-1]
            body = body[:h.start(2)] + val + body[h.end(2):]
        else:
            cut = len(masked.rstrip())
            sep = "" if cut == 0 or masked[cut - 1] == ";" else ";"
            body = body[:cut] + sep + f"\n    --{name}: {val};" + body[cut:]
    return text[:start] + body + text[end:]


FILL_EXCEPTIONS = {"brand-hover", "badge-new-to"}


def compare_fills(old_path, new_path):
    sel = r":root(?:\.dark)?\.theme-[a-z0-9-]+"
    old, new = parse_blocks(old_path, sel), parse_blocks(new_path, sel)
    allowed = set(FIX_ORDER)
    noted, bad = [], []
    for s in sorted(set(old) | set(new)):
        o, n = old.get(s), new.get(s)
        if o is None or n is None:
            bad.append(f"{s}: 选择器{'新增' if o is None else '缺失'}")
            continue
        for k in sorted(set(o) | set(n)):
            if o.get(k) == n.get(k):
                continue
            line = f"{s} {k}: {o.get(k)} -> {n.get(k)}"
            if k[2:] in FILL_EXCEPTIONS:
                noted.append(line)
            elif k[2:] not in allowed:
                bad.append(line)
    for line in noted:
        print("例外:", line)
    for line in bad:
        print("越界:", line)
    print(f"例外 {len(noted)} 项, 越界 {len(bad)} 项")
    return 1 if bad else 0


def suggest_base(variants, bases):
    with open(os.path.join(SRC, "index.css"), encoding="utf-8") as fh:
        text = fh.read()
    for (name, mode, tokens), (label, sel) in zip(variants, ((":root", r":root"), (".dark", r"\.dark"))):
        m = re.search(r"(?m)^\s*" + sel + r"\s*\{", text)
        end = text.index("}", m.end())
        print(f"[{label}] {mode}")
        for k, v in fix_variant(tokens, bases, mode).items():
            hit = re.compile(r"--" + re.escape(k) + r"\s*:").search(text, m.end(), end)
            line = text.count("\n", 0, hit.start()) + 1 if hit else None
            where = f"index.css:{line}" if line else "新增"
            print(f"  --{k}: {tokens.get('--' + k, '(无)')} -> {v}  ({where})")
    return 0


def run_fix(args):
    bases = ["surface", "surface-2"]
    variants = load_variants(args.file, False)
    if args.suggest_base:
        return suggest_base(variants[:2], bases)
    th = parse_blocks(args.file, r":root(?:\.dark)?\.theme-[a-z0-9-]+")
    with open(args.file, encoding="utf-8") as fh:
        text = fh.read()
    for i in range(2, len(variants), 2):
        name = variants[i][0]
        if name == "default":
            continue
        sel_l, sel_d = ":root.theme-" + name, ":root.dark.theme-" + name
        fl = fix_variant(variants[i][2], bases, name + "/light")
        fd = fix_variant(variants[i + 1][2], bases, name + "/dark")
        for k in fl:
            if "--" + k not in th.get(sel_d, {}) and k not in fd:
                fd[k] = variants[i + 1][2]["--" + k]
        text = write_block(text, sel_l, fl)
        text = write_block(text, sel_d, fd)
    out = args.out or args.file
    tmp = out + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(text)
    bad = 0
    for name, mode, tokens in load_variants(tmp, False):
        if name in ("base", "default"):
            continue
        for fg, bg, *_ in check_variant(tokens, FULL_MATRIX, bases, True)[0]:
            print(f"写回后仍失败 {name}/{mode}: {fg} on {bg}")
            bad += 1
    if bad:
        os.remove(tmp)
        return 1
    os.replace(tmp, out)
    print(f"已写入 {out}")
    print("注意: base 与 default 未纳入本次断言, 请另跑 --check 确认整体结果")
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="主题文字对比度审计")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--file", default=os.path.join(SRC, "themes.css"))
    ap.add_argument("--base-only", action="store_true")
    ap.add_argument("--matrix", choices=("legacy", "full"), default="full")
    ap.add_argument("-v", "--verbose", action="store_true")
    ap.add_argument("--fix", action="store_true")
    ap.add_argument("--out")
    ap.add_argument("--suggest-base", action="store_true")
    ap.add_argument("--compare-fills", nargs=2, metavar=("OLD", "NEW"))
    args = ap.parse_args(argv)
    try:
        if args.selftest:
            selftest()
            return 0
        if args.check:
            return run_check(args)
        if args.fix:
            return run_fix(args)
        if args.compare_fills:
            return compare_fills(*args.compare_fills)
    except (ToolError, OSError) as e:
        print(f"工具错误: {e}", file=sys.stderr)
        return 3
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
