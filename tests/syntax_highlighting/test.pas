program HelloWorld;

{ Block comment: demonstrates Pascal / Delphi / FreePascal syntax }
// Line comment

uses
  SysUtils, Classes, Math;

// ── Constants & types ──────────────────────────────────────────────────────

const
  MAX_ITEMS   = 64;
  APP_VERSION = '1.0.0';
  Pi2         = Pi * 2;

type
  TLogLevel = (llDebug, llInfo, llWarning, llError);

  TPoint = record
    X, Y: Double;
  end;

  TNameList = array[0..MAX_ITEMS - 1] of string;

  IGreeter = interface
    procedure Greet(const Name: string);
    function  FormatMessage(const Name: string): string;
  end;

  TGreeter = class(TInterfacedObject, IGreeter)
  private
    FPrefix  : string;
    FCounter : Integer;
  public
    constructor Create(const APrefix: string);
    destructor  Destroy; override;
    procedure   Greet(const Name: string);
    function    FormatMessage(const Name: string): string;
    property    Counter: Integer read FCounter;
    property    Prefix : string  read FPrefix write FPrefix;
  end;

// ── TGreeter implementation ────────────────────────────────────────────────

constructor TGreeter.Create(const APrefix: string);
begin
  inherited Create;
  FPrefix  := APrefix;
  FCounter := 0;
end;

destructor TGreeter.Destroy;
begin
  inherited;
end;

function TGreeter.FormatMessage(const Name: string): string;
begin
  Result := Format('[%s] Hello, %s! (call #%d)', [FPrefix, Name, FCounter + 1]);
end;

procedure TGreeter.Greet(const Name: string);
var
  Msg: string;
begin
  Inc(FCounter);
  Msg := FormatMessage(Name);
  WriteLn(Msg);
end;

// ── Utilities ──────────────────────────────────────────────────────────────

function Distance(const A, B: TPoint): Double;
begin
  Result := Sqrt(Sqr(B.X - A.X) + Sqr(B.Y - A.Y));
end;

procedure LogLevel(Level: TLogLevel; const Msg: string);
const
  Labels: array[TLogLevel] of string = ('DEBUG', 'INFO', 'WARN', 'ERROR');
begin
  WriteLn(Format('%s  %s', [Labels[Level], Msg]));
end;

// ── Main ───────────────────────────────────────────────────────────────────

var
  Greeter : TGreeter;
  Names   : array of string;
  Origin  : TPoint;
  Target  : TPoint;
  I       : Integer;
  Total   : Double;

begin
  Greeter := TGreeter.Create('Demo');
  try
    // Dynamic array and for-loop
    Names := ['Alice', 'Bob', 'Carol', 'Dave'];
    for I := Low(Names) to High(Names) do
      Greeter.Greet(Names[I]);

    // Case statement
    case Greeter.Counter mod 3 of
      0: LogLevel(llDebug,   'divisible by 3');
      1: LogLevel(llInfo,    'remainder 1');
      2: LogLevel(llWarning, 'remainder 2');
    else
      LogLevel(llError, 'unexpected');
    end;

    // Repeat / until
    I := 0;
    repeat
      Inc(I);
    until I >= 5;

    // While loop and arithmetic operators
    Total := 0;
    I     := 1;
    while I <= 10 do
    begin
      if (I mod 2 = 0) and not (I div 2 = 3) then
        Total := Total + I;
      Inc(I);
    end;
    WriteLn(Format('Sum of qualifying evens: %.0f', [Total]));

    // Record and function call
    Origin.X := 0.0;  Origin.Y := 0.0;
    Target.X := 3.0;  Target.Y := 4.0;
    WriteLn(Format('Distance: %.4f', [Distance(Origin, Target)]));

    WriteLn('Version: ' + APP_VERSION);
    WriteLn(Format('Counter: %d', [Greeter.Counter]));
  except
    on E: Exception do
      LogLevel(llError, 'Exception: ' + E.Message);
  finally
    Greeter.Free;
  end;
end.
