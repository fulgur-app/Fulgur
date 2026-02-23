with Ada.Text_IO;         use Ada.Text_IO;
with Ada.Integer_Text_IO; use Ada.Integer_Text_IO;
with Ada.Exceptions;      use Ada.Exceptions;

-- Shape hierarchy demonstrating Ada type system
package Shapes is

   type Color is (Red, Green, Blue, Yellow);

   type Point is record
      X : Float := 0.0;
      Y : Float := 0.0;
   end record;

   type Shape_Kind is (Circle, Rectangle, Triangle);

   type Shape (Kind : Shape_Kind) is record
      Origin : Point;
      Fill   : Color;
      case Kind is
         when Circle =>
            Radius : Float;
         when Rectangle =>
            Width  : Float;
            Height : Float;
         when Triangle =>
            Base      : Float;
            Tri_Height : Float;
      end case;
   end record;

   function Area (S : Shape) return Float;
   function Describe (S : Shape) return String;

end Shapes;

package body Shapes is

   function Area (S : Shape) return Float is
   begin
      case S.Kind is
         when Circle =>
            return Float'Pi * S.Radius ** 2;
         when Rectangle =>
            return S.Width * S.Height;
         when Triangle =>
            return 0.5 * S.Base * S.Tri_Height;
      end case;
   end Area;

   function Describe (S : Shape) return String is
   begin
      return Shape_Kind'Image (S.Kind) & " at ("
         & Float'Image (S.Origin.X) & ", "
         & Float'Image (S.Origin.Y) & ")";
   end Describe;

end Shapes;

-- Generic stack
generic
   type Element_Type is private;
   Capacity : Positive := 64;
package Generic_Stack is

   Stack_Empty : exception;
   Stack_Full  : exception;

   type Stack is limited private;

   procedure Push (S : in out Stack; Item : Element_Type);
   procedure Pop  (S : in out Stack; Item : out Element_Type);
   function  Peek (S : Stack) return Element_Type;
   function  Size (S : Stack) return Natural;

private
   type Buffer is array (1 .. Capacity) of Element_Type;
   type Stack is record
      Data : Buffer;
      Top  : Natural := 0;
   end record;

end Generic_Stack;

-- Main program
procedure Main is
   use Shapes;

   package Int_Stack is new Generic_Stack (Element_Type => Integer, Capacity => 10);
   use Int_Stack;

   S  : Stack;
   Val : Integer;

   function Factorial (N : Positive) return Long_Integer is
   begin
      if N = 1 then
         return 1;
      else
         return Long_Integer (N) * Factorial (N - 1);
      end if;
   end Factorial;

begin
   -- Push values and compute sum
   for I in 1 .. 5 loop
      Push (S, I * I);
   end loop;

   Put_Line ("Stack size:" & Natural'Image (Size (S)));

   declare
      Total : Integer := 0;
   begin
      while Size (S) > 0 loop
         Pop (S, Val);
         Total := Total + Val;
      end loop;
      Put_Line ("Sum of squares: " & Integer'Image (Total));
   exception
      when Stack_Empty => Put_Line ("Unexpected empty stack");
   end;

   -- Shapes demo
   declare
      C : constant Shape := (Kind   => Circle,
                             Origin => (1.0, 2.0),
                             Fill   => Red,
                             Radius => 3.5);
      R : constant Shape := (Kind   => Rectangle,
                             Origin => (0.0, 0.0),
                             Fill   => Blue,
                             Width  => 4.0,
                             Height => 6.0);
   begin
      for S of Shape'(C, R) loop -- attribute: array aggregate
         Put_Line (Describe (S) & " area =" & Float'Image (Area (S)));
      end loop;
   end;

   -- Factorial with exception guard
   begin
      Put_Line ("10! =" & Long_Integer'Image (Factorial (10)));
   exception
      when Constraint_Error => Put_Line ("Overflow computing factorial");
      when E : others       => Put_Line ("Error: " & Exception_Message (E));
   end;

end Main;
