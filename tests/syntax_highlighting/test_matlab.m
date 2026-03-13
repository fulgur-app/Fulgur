% MATLAB syntax highlighting test file

%% Section: Variables and Basic Types
x = 42;
y = 3.14;
z = true;
name = 'hello world';
dq_name = "double quoted string";
arr = [1, 2, 3, 4, 5];
mat = [1 2 3; 4 5 6; 7 8 9];
cell_arr = {'alpha', 42, true};
empty = [];

%% Section: Arithmetic Operators
a = x + y;
b = x - y;
c = x * y;
d = x / y;
e = x ^ 2;
f = ~z;
g = mat';

%% Section: Control Flow - Conditionals
if x > 0
    disp('positive');
elseif x < 0
    disp('negative');
else
    disp('zero');
end

switch x
    case 1
        disp('one');
    case {2, 3}
        disp('two or three');
    otherwise
        disp('other');
end

%% Section: Control Flow - Loops
for i = 1:10
    if i == 5
        continue;
    end
    if i == 8
        break;
    end
    fprintf('%d\n', i);
end

k = 0;
while k < 5
    k = k + 1;
end

parfor j = 1:4
    result(j) = j * 2;
end

%% Section: Functions
function result = add(a, b)
    result = a + b;
end

function [s, p] = sum_and_product(x, y)
    s = x + y;
    p = x * y;
end

function output = apply(func, value)
    output = func(value);
end

%% Section: Anonymous Functions and Handles
square = @(x) x ^ 2;
add_one = @(x) x + 1;
composed = @(x) square(add_one(x));

%% Section: Error Handling
try
    result = 1 / 0;
catch err
    fprintf('Error: %s\n', err.message);
end

%% Section: Global and Persistent
global shared_counter;
shared_counter = 0;

function increment()
    global shared_counter;
    persistent call_count;
    if isempty(call_count)
        call_count = 0;
    end
    call_count = call_count + 1;
    shared_counter = shared_counter + 1;
end

%% Section: Class Definition
classdef Animal
    properties
        Name
        Sound
    end

    methods
        function obj = Animal(name, sound)
            obj.Name = name;
            obj.Sound = sound;
        end

        function speak(obj)
            fprintf('%s says %s\n', obj.Name, obj.Sound);
        end
    end

    events
        OnSpeak
    end

    enumeration
        Cat ('Meow')
        Dog ('Woof')
    end
end

%% Section: String Formatting
msg = sprintf('Value: %d, Float: %.2f', x, y);
fprintf('Result: %s\n', msg);

%% Section: Matrix Operations
A = magic(3);
B = A';
C = A * B;
[rows, cols] = size(A);
det_A = det(A);
inv_A = inv(A);

%% Section: Logical Operations
flag1 = x > 0 & y > 0;
flag2 = x > 0 | y < 0;
flag3 = ~flag1;
idx = arr > 2;
filtered = arr(idx);

%% Section: Cell and Struct Operations
s.field1 = 'value';
s.field2 = 42;
fields = fieldnames(s);

c = {1, 'two', [3 4 5]};
first = c{1};
second = c{2};

%% Section: Line Continuation
long_result = 1 + 2 + 3 + ...
              4 + 5 + 6;

%% Section: Ignored Output
[~, idx] = max(arr);
