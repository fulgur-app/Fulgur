// Groovy syntax highlighting test

// ── Imports ───────────────────────────────────────────────────────────────────

import groovy.transform.Immutable
import groovy.transform.ToString

// ── GString and multiline strings ─────────────────────────────────────────────

def name = 'Groovy'
def version = 4.0
def greeting = "Hello from ${name} ${version}!"
def multiline = """\
    Language: ${name}
    Version:  ${version}
    Type:     ${name.class.simpleName}
"""

// ── Enum ──────────────────────────────────────────────────────────────────────

enum Priority {
    LOW, MEDIUM, HIGH

    String describe() { "Priority level: ${name().toLowerCase()}" }
}

// ── Immutable value class ─────────────────────────────────────────────────────

@Immutable
@ToString(includeNames = true)
class Point {
    double x
    double y

    double distanceTo(Point other) {
        Math.sqrt((x - other.x) ** 2 + (y - other.y) ** 2)
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

trait Printable {
    abstract String format()

    void printFormatted() { println "[${this.class.simpleName}] ${format()}" }
}

// ── Class with dynamic and optional typing ────────────────────────────────────

class Task implements Printable {
    String title
    Priority priority = Priority.MEDIUM
    boolean done = false

    def complete() {
        done = true
        this
    }

    @Override
    String format() { "${title} [${priority}]${done ? ' ✓' : ''}" }
}

// ── Closures ──────────────────────────────────────────────────────────────────

def multiply = { int a, int b -> a * b }

def applyTwice = { Closure fn, x -> fn(fn(x)) }

def addN = { int n -> { int x -> x + n } }
def addFive = addN(5)

// ── Collections and Groovy collection methods ─────────────────────────────────

def tasks = [
    new Task(title: 'Write tests',   priority: Priority.HIGH),
    new Task(title: 'Fix bug #42',   priority: Priority.HIGH),
    new Task(title: 'Update docs',   priority: Priority.LOW),
    new Task(title: 'Code review',   priority: Priority.MEDIUM),
    new Task(title: 'Deploy to prod',priority: Priority.MEDIUM),
]

tasks[0].complete()
tasks[1].complete()

def pending   = tasks.findAll({ !it.done })
def byPrio    = tasks.groupBy({ it.priority })
def titles    = tasks*.title
def highCount = tasks.count({ it.priority == Priority.HIGH })

// ── Spread, ranges, and safe navigation ───────────────────────────────────────

def scores = [85, 92, 78, 95, 60, 88]
def (min, max) = [scores.min(), scores.max()]
def average = scores.sum() / scores.size()
def passing = scores.findAll({ it >= 75 }).sort(false)

def range = (1..5).collect({ it ** 2 })

Task maybeNull = null
def safeTitle = maybeNull?.title ?: 'untitled'

// ── Pattern matching with switch ──────────────────────────────────────────────

def classify = { score ->
    switch (score) {
        case 90..100: return 'A'
        case 75..<90: return 'B'
        case 60..<75: return 'C'
        default:      return 'F'
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

println greeting
println multiline.stripIndent()

def origin = new Point(x: 0, y: 0)
def p = new Point(x: 3, y: 4)
println "Distance: ${origin.distanceTo(p)}"

pending.each({ it.printFormatted() })
println "High-priority tasks: ${highCount}"
println "Passing scores:       ${passing}"
println "Squares 1–5:          ${range}"
println "Grades: ${scores.collect({ classify(it) })}"
println "addFive(10) = ${addFive(10)}"
println "applyTwice(×2, 3) = ${applyTwice(multiply.curry(2), 3)}"
