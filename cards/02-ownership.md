# 所有权 Ownership

所有权是 Rust 最核心的概念:**每一个值都有一个所有者**。

## 三条规则

1. 每个值有且仅有一个所有者
2. 当所有者离开作用域,值被丢弃
3. 所有权可以转移(移动),不能复制

## 移动语义

```rust
let s1 = String::from("hello");
let s2 = s1;       // s1 被移动,不能再使用
// println!("{}", s1); // 编译错误
```

## 与 Copy 类型的区别

- 基础类型(整数、浮点、布尔)实现 `Copy`,赋值是拷贝
- 堆上类型(`String`、`Vec`)是移动

## 函数传参

```rust
fn take(s: String) { }        // 传入即交出所有权
fn give() -> String { String::from("x") }  // 返回即转移所有权
```
