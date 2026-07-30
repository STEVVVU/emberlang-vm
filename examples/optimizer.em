fn main() -> int {
    // The IR folds this expression and removes its dead intermediates.
    let answer: int = (2 + 3) * (10 - 2);
    let unused: int = 100 + 200;
    print(answer);
    return 0;
}

