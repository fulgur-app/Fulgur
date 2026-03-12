! Fortran syntax highlighting test file
! Exercises keywords, types, functions, subroutines, operators, literals

program main
    implicit none

    integer :: i, n
    real(8) :: x, y, result
    logical :: flag
    character(len=64) :: message

    n = 10
    x = 3.14159265358979_8
    y = 2.71828182845905_8
    flag = .true.
    message = "Hello, Fortran!"

    print *, message
    print *, "x =", x, "y =", y

    result = compute_sum(x, y)
    print *, "Sum:", result

    result = factorial(n)
    print *, "Factorial of", n, "=", result

    call greet("World")

    do i = 1, 5
        if (i == 3) then
            print *, "Found three"
        else if (i > 3) then
            print *, "Greater than three:", i
        else
            print *, "Less than three:", i
        end if
    end do

    if (flag .and. x > 0.0) then
        print *, "Flag is true and x is positive"
    end if

    call vector_operations()

end program main


! Pure function: compute the sum of two reals
function compute_sum(a, b) result(total)
    real(8), intent(in) :: a, b
    real(8) :: total

    total = a + b
end function compute_sum


! Recursive function: factorial
recursive function factorial(n) result(res)
    integer, intent(in) :: n
    integer :: res

    if (n <= 1) then
        res = 1
    else
        res = n * factorial(n - 1)
    end if
end function factorial


! Subroutine with a character argument
subroutine greet(name)
    character(len=*), intent(in) :: name
    print *, "Hello, " // trim(name) // "!"
end subroutine greet


! Subroutine demonstrating arrays and do loops
subroutine vector_operations()
    implicit none

    integer, parameter :: N = 5
    real(8), dimension(N) :: vec_a, vec_b, vec_c
    integer :: i

    do i = 1, N
        vec_a(i) = real(i, 8)
        vec_b(i) = real(N - i + 1, 8)
    end do

    vec_c = vec_a + vec_b

    print *, "Vector sum:"
    do i = 1, N
        print *, "  vec_c(", i, ") =", vec_c(i)
    end do

    print *, "Dot product:", dot_product(vec_a, vec_b)
end subroutine vector_operations


! Module demonstrating derived types and module procedures
module geometry
    implicit none

    type :: point
        real(8) :: x
        real(8) :: y
    end type point

    type :: circle
        type(point) :: center
        real(8) :: radius
    end type circle

contains

    function distance(p1, p2) result(d)
        type(point), intent(in) :: p1, p2
        real(8) :: d

        d = sqrt((p2%x - p1%x)**2 + (p2%y - p1%y)**2)
    end function distance

    function circle_area(c) result(area)
        type(circle), intent(in) :: c
        real(8), parameter :: PI = 3.14159265358979_8
        real(8) :: area

        area = PI * c%radius**2
    end function circle_area

    subroutine print_circle(c)
        type(circle), intent(in) :: c
        print *, "Circle center: (", c%center%x, ",", c%center%y, ")"
        print *, "Circle radius:", c%radius
        print *, "Circle area:", circle_area(c)
    end subroutine print_circle

end module geometry
