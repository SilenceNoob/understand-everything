# Rust 入门

Rust 是一门**系统级编程语言**,以**内存安全**和**并发安全**著称。

## 为什么学 Rust

- 无需垃圾回收器,也能保证内存安全
- 零成本抽象
- 强大的类型系统和模式匹配
- 出色的错误处理

> 核心思想:所有权系统让编译器替你管理内存。

## 学习路径

1. 所有权(Ownership)
2. 借用(Borrowing)与引用
3. 生命周期(Lifetime)

## 示例代码

```rust
fn main() {
    let s = String::from("hello");
    println!("{}", s);
}
```

## 两张图

![学习路径图](path.png)

这是 PNG 图片(异步解码)。

![示例图片](tiger.svg)

这是 SVG 图片,由 makepad 原生矢量引擎同步解析渲染。
