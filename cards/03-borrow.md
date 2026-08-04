# 借用与引用

**借用(Borrowing)**:不转移所有权,只临时借用。

## 引用规则

- 任意时刻,要么有**一个**可变引用,要么有**多个**不可变引用
- 引用必须始终有效(不能悬垂)

## 示例

```rust
fn len(s: &String) -> usize {
    s.len()          // 借用,不拥有
}

fn main() {
    let s = String::from("hello");
    let n = len(&s); // 传引用
    println!("{} {}", s, n);
}
```

## 可变引用

```rust
let mut s = String::from("hello");
let r = &mut s;      // 唯一的可变借用
r.push_str(", world");
```

> 一句话:**借用者不能超过所有者**,否则编译不过。
