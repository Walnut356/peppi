(ns peppi-codegen.common
  (:require
   [clojure.data.json :as json]
   [clojure.java.io :as io]
   [clojure.pprint :refer [pprint]]
   [clojure.string :as str]))

(def do-not-edit
  "// This file is auto-generated by `gen/scripts/regen`. Do not edit.\n")

(def types
  {"bool" "DataType::Boolean"
   "i8"   "DataType::Int8"
   "u8"   "DataType::UInt8"
   "i16"  "DataType::Int16"
   "u16"  "DataType::UInt16"
   "i32"  "DataType::Int32"
   "u32"  "DataType::UInt32"
   "i64"  "DataType::Int64"
   "u64"  "DataType::UInt64"
   "f32"  "DataType::Float32"
   "f64"  "DataType::Float64"})

(def reserved-idents
  #{"type"})

(defmacro assert!
  ([x]
   (if *assert*
     `(or ~x (throw (new AssertionError (str "Assert failed: " (pr-str '~x)))))
     `(do ~x)))
  ([x msg]
   (if *assert*
     `(or ~x (throw (new AssertionError (str "Assert failed: " ~msg "\n" (pr-str '~x)))))
     `(do ~x))))

(defn kv
  [k v]
  (clojure.lang.MapEntry. k v))

(defn append
  [x coll]
  {:pre [(vector? coll)]}
  (conj coll x))

(defn normalize
  [[nm & more]]
  (let [[props children]
        (if (map? (first more))
          [(first more) (rest more)]
          [{} more])]
    (into [nm props] children)))

(defn pget
  [m k]
  (-> m normalize (get-in [1 k])))

(defn passoc
  [m k v]
  (-> m normalize (assoc-in [1 k] v)))

(def tuple-struct?
  (complement (comp :name first)))

(defn wrap-transpose
  [call]
  [:method-call {:unwrap true} call "transpose"])

(defn wrap-map
  [target binding-name method-call]
  (let [map-call [:method-call
                  target
                  "map"
                  [[:closure
                    [[binding-name]]
                    [(passoc method-call :unwrap false)]]]]]
    (cond-> map-call
      (pget method-call :unwrap) wrap-transpose)))

(defn as-mut
  [x]
  [:method-call x "as_mut"])

(defn as-ref
  [x]
  [:method-call x "as_ref"])

(defn unwrap
  [x]
  [:method-call x "unwrap"])

(defn if-ver
  ([ver then]
   (if-ver ver then nil))
  ([ver then else]
   [:if
    [:method-call "version" "gte" ver]
    (cond->> then
      (not= :block (first then)) (conj [:block]))
    (cond->> else
      (and else (not= :block (first else))) (conj [:block]))]))

(defn nested-version-ifs
  [f fields]
  (->> fields
       (partition-by :version)
       reverse
       (reduce (fn [acc fields]
                 (let [ver (:version (first fields))
                       stmts (concat (mapv f fields) acc)]
                   (if ver
                     [(if-ver ver (into [:block] stmts))]
                     stmts)))
               [])))

;;;
;;; AST emitters
;;;

(defmulti emit-expr*
  (fn [props & _]
    (:type props)))

(defn emit-expr
  [m]
  (cond
    (nil? m) ""
    (string? m) m
    (number? m) m
    :else
    (let [[ty props & children] (normalize m)]
      (apply emit-expr* (assoc props :type ty) children))))

(defn emit-ident
  [ident]
  (cond->> ident
    (reserved-idents ident) (str "r#")))

(defn emit-type
  [x]
  (cond
    (or (string? x) (keyword? x)) x
    (vector? x) (let [[ty & generics] x]
                  (format "%s::<%s>"
                          ty
                          (str/join ", " (mapv emit-type generics))))
    (list? x) (str/join "::"
                        (concat (butlast x)
                                [(emit-type (last x))]))))

(defn emit-fn-body
  [statements]
  (->> statements
       (mapv emit-expr)
       (str/join ";\n")))

(defn emit-fn-arg
  [[nm ty]]
  (if ty
    (format "%s: %s" nm (emit-type ty))
    (emit-ident nm)))

(defmethod emit-expr* :raw
  [_ source]
  source)

(defmethod emit-expr* :unit
  [_]
  "()")

(defmethod emit-expr* :string
  [_ s]
  (format "\"%s\"" s)) ; FIXME: escape

(defmethod emit-expr* :block
  [_ & stmts]
  (->> stmts
       (mapv emit-expr)
       (str/join ";\n")
       (format "{ %s }")))

(defmethod emit-expr* :if
  [_ expr then & [else]]
  (str "if "
       (emit-expr expr)
       (emit-expr then)
       (some->> else emit-expr (str "else "))))

(defmethod emit-expr* :op
  [_ op lhs rhs]
  (format "(%s %s %s)" (emit-expr lhs) op (emit-expr rhs)))

(defmethod emit-expr* :subscript
  [_ target idx]
  (format "%s[%s]" (emit-expr target) idx))

(defmethod emit-expr* :field-get
  [_ target field]
  (format "%s.%s"
          (emit-expr target)
          (emit-ident field)))

(defmethod emit-expr* :method-call
  ([props target nm]
   (emit-expr* props target nm []))
  ([{:keys [generics unwrap]} target nm args]
   (format "%s.%s%s(%s)%s"
           (emit-expr target)
           nm
           (or (some->> generics
                        (mapv emit-type)
                        (str/join ", ")
                        (format "::<%s>"))
               "")
           (->> args
                (mapv emit-expr)
                (str/join ","))
           (if unwrap "?" ""))))

(defmethod emit-expr* :fn-call
  [{:keys [unwrap]} target nm args]
  (format "%s%s(%s)"
          (or (some-> target emit-type (str "::")) "")
          nm
          (->> args
               (mapv emit-expr)
               (str/join ", "))
          (if unwrap "?" "")))

(defmethod emit-expr* :vec!
  [_ args]
  (format "vec![%s]"
          (->> args
               (mapv emit-expr)
               (str/join ", "))))

(defn emit-struct-field-init
  [[nm ty]]
  (format "%s: %s" (emit-ident nm) (emit-expr ty)))

(defmethod emit-expr* :struct-init
  [_ ty fields]
  (if (ffirst fields)
    (format "%s { %s }" ; normal struct
            (emit-type ty)
            (->> fields
                 (mapv emit-struct-field-init)
                 (str/join ", ")))
    (format "%s ( %s )" ; tuple struct
            (emit-type ty)
            (->> fields
                 (mapv (comp emit-expr second))
                 (str/join ", ")))))

#_(defmethod emit-expr* :tuple-struct-init
  [_ nm fields]
  (format "%s(%s)"
          nm
          (->> fields
               (mapv emit-expr)
               (str/join ", "))))

(defmethod emit-expr* :closure
  [_ args body]
  (format "%s { %s }"
          (->> args
               (mapv emit-fn-arg)
               (str/join ", ")
               (format "|%s|"))
          (emit-fn-body body)))

(defmethod emit-expr* :fn
  [{:keys [ret generics visibility]} nm args body]
  {:pre [(= :block (first body))]}
  (format "%s fn %s%s(%s)%s %s\n"
          (or visibility "")
          nm
          (or (some->> generics (str/join ", ") (format "<%s>")) "")
          (->> args
               (mapv emit-fn-arg)
               (str/join ", "))
          (or (some->> ret emit-type (str " -> ")) "")
          (emit-expr body)))

(defmethod emit-expr* :impl
  [props ty fns]
  (->> fns
       (mapv emit-expr)
       (str/join "\n")
       (format "impl %s%s {\n%s\n}"
               (emit-type ty)
               (or (some->> props :for emit-type (str " for "))
                   ""))))

(defn emit-struct-field
  [[nm ty]]
  (format "\tpub %s: %s," (emit-ident nm) (emit-type ty)))

(defn emit-tuple-struct-field
  [[_ ty]]
  (format "\tpub %s," (emit-type ty)))

(defn enum-item
  [[nm value]]
  (format "%s = %s" nm value))

(defn emit-attr
  [[nm args]]
  (format "#[%s%s]"
          (name nm)
          (or (some->> args not-empty (str/join ", ") (format "(%s)"))
              "")))

(defn emit-attrs
  [attrs]
  (->> attrs
       (mapv emit-attr)
       (str/join "\n")))

(defmethod emit-expr* :enum
  [{:keys [attrs]} nm items]
  (format "%s\npub enum %s { %s }"
          (emit-attrs attrs)
          nm
          (str/join ", " (mapv enum-item items))))

(defmethod emit-expr* :struct
  [{:keys [attrs]} nm fields]
  (let [decl (format "%s\npub struct %s"
                     (emit-attrs attrs)
                     nm)]
    (if (ffirst fields)
      (->> fields ; normal struct
           (mapv emit-struct-field)
           (str/join "\n")
           (format "%s {\n%s\n}" decl))
      (->> fields ; tuple struct
           (mapv emit-tuple-struct-field)
           (str/join "\n")
           (format "%s (\n%s\n);" decl)))))

(defmethod emit-expr* :let
  [props nm expr]
  (format "let%s %s = %s;"
          (if (:mutable props)
            " mut"
            "")
          (if (coll? nm)
            (format "(%s)" (str/join ", " nm))
            nm)
          (emit-expr expr)))

(defn read-json
  [path]
  (-> path
      io/reader
      (json/read :key-fn keyword, :bigdec true)
      (update-keys name)
      (->> (sort-by key))))