fn classify(value: int) -> int {
    if value < 0 {
        return -1;
    } else {
        if value == 0 {
            return 0;
        } else {
            return 1;
        }
    }
}

fn main() -> int {
    print(classify(-4));
    print(classify(0));
    print(classify(9));
    return 0;
}

