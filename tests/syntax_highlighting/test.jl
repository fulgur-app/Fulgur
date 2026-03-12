module MathUtils

export fibonacci, Point, Shape, Circle, area

using LinearAlgebra
import Base: show, +

# Abstract type hierarchy
abstract type Shape end

primitive type Float24 24 end

mutable struct Circle <: Shape
    center::Point
    radius::Float64
end

struct Point{T <: Number}
    x::T
    y::T
end

const ORIGIN = Point(0.0, 0.0)
const PI_APPROX = 3.14159265

# Multiple dispatch
area(c::Circle)::Float64 = PI_APPROX * c.radius^2
area(width::Float64, height::Float64) = width * height

function fibonacci(n::Int)::Int
    if n <= 1
        return n
    end
    a, b = 0, 1
    for i in 2:n
        a, b = b, a + b
    end
    return b
end

function Base.show(io::IO, p::Point)
    print(io, "($(p.x), $(p.y))")
end

function +(p1::Point{T}, p2::Point{T}) where {T}
    return Point(p1.x + p2.x, p1.y + p2.y)
end

# Enum-like pattern
@enum Color red green blue

# Macro definition
macro assert_positive(expr)
    return quote
        val = $(esc(expr))
        val > 0 || throw(ArgumentError("Expected positive value, got $val"))
        val
    end
end

# Higher-order functions and closures
function apply_transform(points::Vector{Point{T}}, f::Function) where {T}
    return map(f, points)
end

scale = (factor) -> (p) -> Point(p.x * factor, p.y * factor)

# Exception handling
function safe_divide(a::Number, b::Number)
    try
        result = a / b
        if isnan(result) || isinf(result)
            throw(DomainError(b, "Division produced NaN or Inf"))
        end
        return result
    catch e
        if isa(e, DivideError)
            return nothing
        end
        rethrow(e)
    finally
        @debug "Division attempted: $a / $b"
    end
end

# Comprehensions and generators
squares = [x^2 for x in 1:10]
even_squares = [x^2 for x in 1:20 if x % 2 == 0]
matrix = [i + j for i in 1:3, j in 1:3]

# String operations
greeting = "Hello, Julia!"
raw_str = raw"No \n escaping here"
multiline = """
    This is a
    multiline string
"""
regex_pattern = r"^\d{3}-\d{4}$"

# Type unions and parametric types
const MaybeInt = Union{Int, Nothing}
const Numeric = Union{Int, Float64, Complex{Float64}}

# Do-block syntax
results = map(1:5) do x
    x^2 + 2x + 1
end

# Broadcasting
values = [1.0, 2.0, 3.0, 4.0]
scaled = sin.(values) .+ cos.(values)
clamped = clamp.(values, 0.0, 2.5)

# Let block
let
    local_var = 42
    global_result = local_var * 2
end

# While loop
function collatz(n::Int)
    steps = 0
    while n != 1
        n = n % 2 == 0 ? n / 2 : 3n + 1
        steps += 1
    end
    return steps
end

end # module
