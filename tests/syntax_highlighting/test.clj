(ns shapes.core
  (:require [clojure.string :as str]
            [clojure.set    :as set]))

;; ----- Protocols and records -----

(defprotocol Shape
  "Geometric shape abstraction."
  (area      [s] "Compute the area.")
  (perimeter [s] "Compute the perimeter.")
  (describe  [s] "Return a human-readable string."))

(defrecord Circle [radius]
  Shape
  (area      [_] (* Math/PI radius radius))
  (perimeter [_] (* 2.0 Math/PI radius))
  (describe  [_] (str "Circle(r=" radius ")")))

(defrecord Rectangle [width height]
  Shape
  (area      [_] (* width height))
  (perimeter [_] (* 2 (+ width height)))
  (describe  [_] (str "Rectangle(" width "×" height ")")))

;; ----- Multimethods -----

(defmulti  preferred-color :kind)
(defmethod preferred-color :circle    [_] :red)
(defmethod preferred-color :rectangle [_] :blue)
(defmethod preferred-color :default   [_] :grey)

;; ----- Atoms and mutable state -----

(def ^:private registry (atom {}))

(defn register! [id shape]
  (swap! registry assoc id shape))

(defn clear! []
  (reset! registry {}))

;; ----- Destructuring -----

(defn summarise
  "Build a summary map for a shape, accepting an options map."
  [{:keys [label] :or {label "unnamed"}} shape]
  {:label     label
   :area      (area shape)
   :perimeter (perimeter shape)
   :desc      (describe shape)})

;; ----- Threading macros -----

(defn top-n-by-area
  "Return descriptions of the n largest shapes."
  [n shapes]
  (->> shapes
       (sort-by area >)
       (take n)
       (map describe)))

;; ----- Custom macro -----

(defmacro timed
  "Execute body, print elapsed time in ms, and return the result."
  [label & body]
  `(let [t0#     (System/currentTimeMillis)
         result# (do ~@body)
         elapsed# (- (System/currentTimeMillis) t0#)]
     (println ~label "took" elapsed# "ms")
     result#))

;; ----- Lazy sequences and letfn -----

(defn fibonacci
  "Returns a lazy infinite Fibonacci sequence."
  []
  (letfn [(fib [a b] (lazy-seq (cons a (fib b (+ a b)))))]
    (fib 0N 1N)))

;; ----- loop / recur -----

(defn factorial [n]
  (loop [i n acc 1]
    (if (zero? i)
      acc
      (recur (dec i) (*' i acc)))))

;; ----- Regex, metadata, and set literals -----

(def ^:const version "1.0.0")
(def ^String email-re #"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}")

(def reserved-labels #{"unnamed" "unknown" "n/a"})

(defn valid-email? [s]
  (boolean (re-matches email-re s)))

;; ----- Entry point -----

(defn -main [& _args]
  (let [shapes [(->Circle 3.0)
                (->Rectangle 4.0 5.0)
                (->Circle 1.5)]]

    (run! #(register! (gensym "s") %) shapes)

    (doseq [s shapes]
      (println (summarise {:label "demo"} s)))

    (println "Top 2 by area:" (top-n-by-area 2 shapes))
    (println "Fib(10):"       (take 10 (fibonacci)))
    (println "10!:"           (factorial 10))
    (println "Valid email?"   (valid-email? "user@example.com"))
    (println "22/7 ="         (/ 22 7))
    (println "Registry size:" (count @registry))))
