// Iterative Fibonacci demonstrates functions, loops, mutation, and output.
fn fibonacci(n: int) -> int {
    let previous: int = 0;
    let current: int = 1;
    let index: int = 0;

    while index < n {
        let next: int = previous + current;
        previous = current;
        current = next;
        index = index + 1;
    }

    return previous;
}

fn main() -> int {
    print(fibonacci(10));
    return 0;
}

