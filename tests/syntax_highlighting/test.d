module example;

import std.stdio;
import std.algorithm;
import std.array;
import std.string : format, toUpper;
import std.conv : to;

// Constants and enums

enum Color { Red, Green, Blue }

enum uint MAX_SIZE = 1024;

immutable string APP_NAME = "Fulgur";

// Structs and interfaces

interface Shape {
    double area() const;
    double perimeter() const;
}

struct Point {
    double x;
    double y;

    double distanceTo(const Point other) const {
        import std.math : sqrt;
        return sqrt((x - other.x) ^^ 2 + (y - other.y) ^^ 2);
    }
}

class Circle : Shape {
    private double _radius;
    private Point _center;

    this(double radius, Point center) {
        _radius = radius;
        _center = center;
    }

    @property double radius() const { return _radius; }

    override double area() const {
        import std.math : PI;
        return PI * _radius * _radius;
    }

    override double perimeter() const {
        import std.math : PI;
        return 2 * PI * _radius;
    }
}

// Templates

T max(T)(T a, T b) {
    return a > b ? a : b;
}

T[] filter(T)(T[] arr, bool delegate(T) predicate) {
    return arr.filter!(predicate).array;
}

// Ranges and UFCS

int[] doubleAll(int[] values) {
    return values.map!(x => x * 2).array;
}

int[] evens(int[] values) {
    return values.filter!(x => x % 2 == 0).array;
}

// String operations

string greet(string name) {
    return format("Hello, %s!", name);
}

// Exception handling

int safeDivide(int x, int y) {
    if (y == 0) {
        throw new Exception("Division by zero");
    }
    return x / y;
}

int trySafeDivide(int x, int y) {
    try {
        return safeDivide(x, y);
    } catch (Exception e) {
        writeln("Error: ", e.msg);
        return 0;
    } finally {
        writeln("safeDivide attempted");
    }
}

// Mixins and compile-time features

mixin template Logging() {
    void log(string msg) {
        writeln("[LOG] ", msg);
    }
}

class Service {
    mixin Logging;

    void run() {
        log("Service started");
    }
}

// Unit tests

unittest {
    assert(max(3, 5) == 5);
    assert(max(10, 2) == 10);

    auto c = new Circle(3.0, Point(0, 0));
    import std.math : approxEqual;
    assert(approxEqual(c.area(), 28.274, 0.001));
}

// Main entry point

void main() {
    auto nums = [1, 2, 3, 4, 5, 6];
    writeln(greet(APP_NAME));
    writeln("Evens: ", evens(nums));
    writeln("Doubled: ", doubleAll(nums));
    writeln("Max: ", max(42, 17));
    writeln("Color: ", Color.Green);

    auto svc = new Service();
    svc.run();
}
