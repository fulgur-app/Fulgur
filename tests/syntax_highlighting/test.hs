-- Haskell syntax highlighting test

module Main where

import Data.List (sort, group, nub, intercalate)
import Data.Char (toUpper, isAlpha)
import Data.Maybe (fromMaybe, mapMaybe)

-- ── Type aliases and newtypes ──────────────────────────────────────────────

type Name = String
type Score = Int

newtype Email = Email { unEmail :: String }
  deriving (Show, Eq)

-- ── Algebraic data types ───────────────────────────────────────────────────

data Priority = Low | Medium | High
  deriving (Show, Eq, Ord, Enum, Bounded)

data Shape
  = Circle    { radius :: Double }
  | Rectangle { width :: Double, height :: Double }
  | Triangle  Double Double Double
  deriving (Show)

data Tree a = Leaf | Node (Tree a) a (Tree a)

-- ── Type class instances ───────────────────────────────────────────────────

class Describable a where
  describe :: a -> String

instance Describable Shape where
  describe (Circle r)        = "Circle with radius "    ++ show r
  describe (Rectangle w h)   = "Rectangle "             ++ show w ++ "x" ++ show h
  describe (Triangle a b c)  = "Triangle with sides "   ++ intercalate ", " (map show [a, b, c])

instance Describable Priority where
  describe p = "Priority: " ++ show p

-- ── Functions with pattern matching ───────────────────────────────────────

area :: Shape -> Double
area (Circle r)       = pi * r * r
area (Rectangle w h)  = w * h
area (Triangle a b c) = let s = (a + b + c) / 2
                         in sqrt (s * (s-a) * (s-b) * (s-c))

-- ── Recursive data structure operations ───────────────────────────────────

insert :: Ord a => a -> Tree a -> Tree a
insert x Leaf = Node Leaf x Leaf
insert x (Node l v r)
  | x < v    = Node (insert x l) v r
  | x > v    = Node l v (insert x r)
  | otherwise = Node l v r

toList :: Tree a -> [a]
toList Leaf         = []
toList (Node l v r) = toList l ++ [v] ++ toList r

fromList :: Ord a => [a] -> Tree a
fromList = foldr insert Leaf

-- ── Higher-order functions and function composition ────────────────────────

capitalize :: String -> String
capitalize []     = []
capitalize (c:cs) = toUpper c : cs

wordFrequency :: String -> [(String, Int)]
wordFrequency = map (\ws -> (head ws, length ws))
              . group
              . sort
              . words
              . filter (\c -> isAlpha c || c == ' ')

topN :: Int -> [(String, Int)] -> [(String, Int)]
topN n = take n . reverse . sort . map (\(w, c) -> (w, c))

-- ── Maybe and list comprehensions ─────────────────────────────────────────

safeHead :: [a] -> Maybe a
safeHead []    = Nothing
safeHead (x:_) = Just x

safeDivide :: Double -> Double -> Maybe Double
safeDivide _ 0 = Nothing
safeDivide x y = Just (x / y)

parseScore :: String -> Maybe Score
parseScore s = case reads s of
  [(n, "")] | n >= 0 && n <= 100 -> Just n
  _                               -> Nothing

validScores :: [String] -> [Score]
validScores = mapMaybe parseScore

pythagoreanTriples :: Int -> [(Int, Int, Int)]
pythagoreanTriples n =
  [ (a, b, c)
  | c <- [1..n]
  , b <- [1..c]
  , a <- [1..b]
  , a*a + b*b == c*c
  ]

-- ── Entry point ───────────────────────────────────────────────────────────

main :: IO ()
main = do
  let shapes = [Circle 3.0, Rectangle 4.0 5.0, Triangle 3.0 4.0 5.0]
  putStrLn "=== Shapes ==="
  mapM_ (\s -> putStrLn $ describe s ++ " | area = " ++ show (area s)) shapes

  let bst = fromList [5, 3, 7, 1, 4, 6, 8 :: Int]
  putStrLn $ "\nBST in-order: " ++ show (toList bst)

  let text = "the cat sat on the mat the cat"
      freq = topN 3 (wordFrequency text)
  putStrLn $ "\nTop words: " ++ show freq

  let inputs = ["42", "101", "85", "abc", "0"]
  putStrLn $ "Valid scores: " ++ show (validScores inputs)

  putStrLn $ "Pythagorean triples up to 20: " ++ show (pythagoreanTriples 20)

  let result = do
        x <- safeDivide 10.0 3.0
        y <- safeDivide x 2.0
        return (fromMaybe 0.0 (safeDivide y 0.0))
  putStrLn $ "\nChained division: " ++ show result
