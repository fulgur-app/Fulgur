# R syntax highlighting test — statistical computing showcase

library(stats)

# ── Constants and special values ──────────────────────────────────────────────

MAX_ITER <- 1000L
TOLERANCE <- 1e-8
GREETING <- "Hello, R!"

empty <- NULL
missing_val <- NA_real_
inf_val <- Inf
not_a_number <- NaN

# ── Vectors and basic operations ──────────────────────────────────────────────

scores <- c(82, 91, 74, 88, 95, 67, 79)
names(scores) <- c("Alice", "Bob", "Carol", "Dave", "Eve", "Frank", "Grace")

above_mean <- scores[scores > mean(scores)]
scaled <- (scores - min(scores)) / (max(scores) - min(scores))

seq_evens <- seq(2, 20, by = 2)
repeated <- rep(c(1L, 2L, 3L), times = 3)

# ── String operations ─────────────────────────────────────────────────────────

greet <- function(name, punctuation = "!") {
  sprintf("Hello, %s%s", name, punctuation)
}

names_raw <- c("  alice ", "BOB", "carol  ")
cleaned <- trimws(tolower(names_raw))
pattern_match <- grepl("^[aeiou]", cleaned, ignore.case = TRUE)

# ── Control flow ──────────────────────────────────────────────────────────────

classify_score <- function(x) {
  if (is.na(x)) {
    return("missing")
  } else if (x >= 90) {
    "excellent"
  } else if (x >= 75) {
    "good"
  } else {
    "needs improvement"
  }
}

grades <- vapply(scores, classify_score, character(1L))

fizzbuzz <- character(20L)
for (i in seq_along(fizzbuzz)) {
  fizzbuzz[i] <- if (i %% 15 == 0) "FizzBuzz"
                 else if (i %% 3 == 0) "Fizz"
                 else if (i %% 5 == 0) "Buzz"
                 else as.character(i)
}

# ── Apply family and closures ─────────────────────────────────────────────────

make_power <- function(n) {
  force(n)
  function(x) x^n
}

square  <- make_power(2)
cube    <- make_power(3)

matrix_data <- matrix(1:12, nrow = 3, ncol = 4)
row_sums <- apply(matrix_data, 1, sum)
col_means <- apply(matrix_data, 2, mean)

nested <- list(a = 1:5, b = 6:10, c = 11:15)
list_means <- sapply(nested, mean)

# ── Data frames ───────────────────────────────────────────────────────────────

df <- data.frame(
  name  = cleaned,
  score = scores[seq_along(cleaned)],
  grade = grades[seq_along(cleaned)],
  stringsAsFactors = FALSE
)

df$pass <- df$score >= 75
top_students <- df[df$pass, c("name", "score")]

# ── S3 class ──────────────────────────────────────────────────────────────────

new_counter <- function(start = 0L) {
  env <- new.env(parent = emptyenv())
  env$value <- as.integer(start)

  structure(env, class = "Counter")
}

increment <- function(x, ...) UseMethod("increment")
increment.Counter <- function(x, by = 1L) {
  x$value <- x$value + as.integer(by)
  invisible(x)
}

print.Counter <- function(x, ...) {
  cat(sprintf("<Counter: %d>\n", x$value))
  invisible(x)
}

ctr <- new_counter(10L)
increment(ctr)
increment(ctr, by = 4L)
print(ctr)

# ── Pipe and formula ──────────────────────────────────────────────────────────

result <- scores |>
  (\(x) x[x > median(x)])() |>
  sort(decreasing = TRUE)

fit <- lm(score ~ 1, data = df)
summary_stats <- summary(fit)

cat(greet("world"), "\n")
cat("Top scores:", paste(result, collapse = ", "), "\n")
