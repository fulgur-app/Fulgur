% Prolog syntax highlighting test file

% Facts
parent(tom, bob).
parent(tom, liz).
parent(bob, ann).
parent(bob, pat).

% Rules
grandparent(X, Z) :-
    parent(X, Y),
    parent(Y, Z).

ancestor(X, Y) :-
    parent(X, Y).

ancestor(X, Y) :-
    parent(X, Z),
    ancestor(Z, Y).

% Arithmetic
factorial(0, 1) :- !.
factorial(N, F) :-
    N > 0,
    N1 is N - 1,
    factorial(N1, F1),
    F is N * F1.

% List operations
my_length([], 0).
my_length([_|T], N) :-
    my_length(T, N1),
    N is N1 + 1.

my_append([], L, L).
my_append([H|T], L, [H|R]) :-
    my_append(T, L, R).

my_member(X, [X|_]).
my_member(X, [_|T]) :-
    my_member(X, T).

my_reverse([], []).
my_reverse([H|T], R) :-
    my_reverse(T, RT),
    my_append(RT, [H], R).

% String operations
greeting(Name, Greeting) :-
    atom_concat('Hello, ', Name, G1),
    atom_concat(G1, '!', Greeting).

% Cuts and negation
max(X, Y, X) :- X >= Y, !.
max(_, Y, Y).

not_member(_, []).
not_member(X, [H|T]) :-
    X \= H,
    not_member(X, T).

% Assert and retract
:- dynamic counter/1.
counter(0).

increment :-
    retract(counter(N)),
    N1 is N + 1,
    assert(counter(N1)).

% Operators
:- op(700, xfx, ===).
X === X.

% Numbers
integer_example(42).
float_example(3.14159).
negative_example(-7).
hex_example(0xff).

% Structures
point(X, Y) :- number(X), number(Y).
distance(point(X1, Y1), point(X2, Y2), D) :-
    DX is X2 - X1,
    DY is Y2 - Y1,
    D is sqrt(DX * DX + DY * DY).

% Strings (double-quoted lists in standard Prolog)
hello_world("Hello, World!").

% If-then-else
classify(N, positive) :- N > 0, !.
classify(N, negative) :- N < 0, !.
classify(0, zero).

% Findall
all_parents(Child, Parents) :-
    findall(P, parent(P, Child), Parents).

% Between
sum_to(N, S) :-
    findall(X, between(1, N, X), Xs),
    sumlist(Xs, S).

% Modules
:- module(geometry, [area/2, perimeter/2]).

area(circle(R), A) :-
    A is pi * R * R.

area(rect(W, H), A) :-
    A is W * H.

perimeter(circle(R), P) :-
    P is 2 * pi * R.

perimeter(rect(W, H), P) :-
    P is 2 * (W + H).

% DCG (Definite Clause Grammars)
sentence --> noun_phrase, verb_phrase.
noun_phrase --> [the], noun.
verb_phrase --> verb, noun_phrase.
verb_phrase --> verb.
noun --> [dog].
noun --> [cat].
verb --> [sees].
verb --> [chases].
