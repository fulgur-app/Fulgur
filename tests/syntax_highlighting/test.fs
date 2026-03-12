// F# syntax highlighting test file
// Exercises modules, types, functions, pattern matching, computation expressions

module Geometry =

    type Point = { X: float; Y: float }

    type Shape =
        | Circle of center: Point * radius: float
        | Rectangle of topLeft: Point * bottomRight: Point
        | Triangle of Point * Point * Point

    let distance (p1: Point) (p2: Point) : float =
        let dx = p2.X - p1.X
        let dy = p2.Y - p1.Y
        sqrt (dx * dx + dy * dy)

    let area shape =
        match shape with
        | Circle (_, r) -> System.Math.PI * r * r
        | Rectangle (tl, br) ->
            let width = abs (br.X - tl.X)
            let height = abs (br.Y - tl.Y)
            width * height
        | Triangle (a, b, c) ->
            let ab = distance a b
            let bc = distance b c
            let ca = distance c a
            let s = (ab + bc + ca) / 2.0
            sqrt (s * (s - ab) * (s - bc) * (s - ca))

    let perimeter shape =
        match shape with
        | Circle (_, r) -> 2.0 * System.Math.PI * r
        | Rectangle (tl, br) ->
            2.0 * (abs (br.X - tl.X) + abs (br.Y - tl.Y))
        | Triangle (a, b, c) ->
            distance a b + distance b c + distance c a


module Collections =

    let filterMap predicate transform xs =
        xs
        |> List.filter predicate
        |> List.map transform

    let sumSquaresOfEvens limit =
        [ 1 .. limit ]
        |> List.filter (fun n -> n % 2 = 0)
        |> List.map (fun n -> n * n)
        |> List.sum

    let groupByFirstLetter (words: string list) =
        words
        |> List.groupBy (fun w -> w.[0])
        |> Map.ofList

    let rec fibonacci n =
        match n with
        | 0 -> 0
        | 1 -> 1
        | n -> fibonacci (n - 1) + fibonacci (n - 2)

    let memoize f =
        let cache = System.Collections.Generic.Dictionary<_, _>()
        fun x ->
            match cache.TryGetValue(x) with
            | true, v -> v
            | false, _ ->
                let v = f x
                cache.[x] <- v
                v


module Async =

    let fetchWithTimeout (url: string) (timeoutMs: int) =
        async {
            use client = new System.Net.Http.HttpClient()
            client.Timeout <- System.TimeSpan.FromMilliseconds(float timeoutMs)
            let! response = client.GetStringAsync(url) |> Async.AwaitTask
            return response
        }

    let runAll computations =
        computations
        |> List.map Async.StartAsTask
        |> System.Threading.Tasks.Task.WhenAll
        |> Async.AwaitTask


[<Literal>]
let MaxRetries = 3

[<Struct>]
type Vector2D =
    val X: float
    val Y: float
    new(x, y) = { X = x; Y = y }

    member this.Length = sqrt (this.X * this.X + this.Y * this.Y)

    static member (+) (a: Vector2D, b: Vector2D) = Vector2D(a.X + b.X, a.Y + b.Y)
    static member (*) (scalar: float, v: Vector2D) = Vector2D(scalar * v.X, scalar * v.Y)


type Result<'T, 'E> =
    | Ok of 'T
    | Error of 'E

let divide (a: float) (b: float) : Result<float, string> =
    if b = 0.0 then Error "division by zero"
    else Ok (a / b)

let safeSqrt x =
    if x < 0.0 then Error "negative input"
    else Ok (sqrt x)

let compute a b =
    divide a b
    |> fun r ->
        match r with
        | Ok v -> safeSqrt v
        | Error e -> Error e


type IShape =
    abstract member Area: float
    abstract member Perimeter: float

type CircleImpl(radius: float) =
    interface IShape with
        member _.Area = System.Math.PI * radius * radius
        member _.Perimeter = 2.0 * System.Math.PI * radius

    member _.Radius = radius
    override _.ToString() = sprintf "Circle(r=%.2f)" radius


module Program =

    [<EntryPoint>]
    let main _ =
        let origin = { X = 0.0; Y = 0.0 }
        let p1 = { X = 3.0; Y = 4.0 }
        printfn "Distance: %f" (Geometry.distance origin p1)

        let shapes = [
            Geometry.Circle(origin, 5.0)
            Geometry.Rectangle({ X = 0.0; Y = 0.0 }, { X = 4.0; Y = 3.0 })
        ]
        shapes |> List.iter (fun s -> printfn "Area: %f" (Geometry.area s))

        let evens = Collections.sumSquaresOfEvens 10
        printfn "Sum of squares of evens up to 10: %d" evens

        let fibs = List.init 10 Collections.fibonacci
        printfn "First 10 Fibonacci numbers: %A" fibs

        0
