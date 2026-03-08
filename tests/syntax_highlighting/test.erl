-module(example).
-behaviour(gen_server).
-author("fulgur").

-export([start_link/0, stop/0, add/2, factorial/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(MAX_INT, 9999).

-record(state, {
    count = 0 :: non_neg_integer(),
    name  = <<>> :: binary()
}).

-type result() :: {ok, term()} | {error, term()}.

%% Public API

-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-spec stop() -> ok.
stop() ->
    gen_server:call(?SERVER, stop).

-spec add(integer(), integer()) -> integer().
add(X, Y) ->
    X + Y.

%% Recursive function

-spec factorial(non_neg_integer()) -> pos_integer().
factorial(0) -> 1;
factorial(N) when N > 0 ->
    N * factorial(N - 1).

%% Pattern matching and guards

classify(X) when X < 0    -> negative;
classify(0)               -> zero;
classify(X) when X > 0    -> positive.

%% List comprehension and higher-order functions

evens(List) ->
    [X || X <- List, X rem 2 =:= 0].

double_all(List) ->
    lists:map(fun(X) -> X * 2 end, List).

sum(List) ->
    lists:foldl(fun(X, Acc) -> X + Acc end, 0, List).

%% Binaries and strings

greet(Name) when is_binary(Name) ->
    <<"Hello, ", Name/binary, "!">>;
greet(Name) when is_list(Name) ->
    "Hello, " ++ Name ++ "!".

parse_int(Bin) ->
    case binary_to_integer(Bin) of
        N when N >= 0 -> {ok, N};
        _             -> {error, negative}
    end.

%% Try/catch and exception handling

safe_divide(_, 0) ->
    {error, division_by_zero};
safe_divide(X, Y) ->
    try
        {ok, X / Y}
    catch
        error:badarith -> {error, arithmetic_error};
        _:Reason       -> {error, Reason}
    end.

%% gen_server callbacks

init([]) ->
    {ok, #state{}}.

handle_call(stop, _From, State) ->
    {stop, normal, ok, State};
handle_call({add, X, Y}, _From, State) ->
    Result = add(X, Y),
    NewCount = State#state.count + 1,
    {reply, Result, State#state{count = NewCount}};
handle_call(_Request, _From, State) ->
    {reply, {error, unknown_request}, State}.

handle_cast(_Msg, State) ->
    {noreply, State}.

handle_info(_Info, State) ->
    {noreply, State}.

terminate(_Reason, _State) ->
    ok.

code_change(_OldVsn, State, _Extra) ->
    {ok, State}.

%% Atoms, tuples, and maps

describe(Color) ->
    Colors = #{
        red   => {255, 0, 0},
        green => {0, 255, 0},
        blue  => {0, 0, 255}
    },
    maps:get(Color, Colors, {0, 0, 0}).

is_weekend(Day) ->
    lists:member(Day, [saturday, sunday]).
