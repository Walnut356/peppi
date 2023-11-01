(ns peppi-codegen.frame.transpose
  (:require
   [clojure.java.io :as io]
   [peppi-codegen.common :refer :all]))

(defn field
  [{nm :name, ty :type, ver :version}]
  [nm (cond->> ty
        ver (conj ["Option"]))])

(defn struct-decl
  [[nm fields]]
  [:struct
   {:attrs {:derive ["PartialEq" "Clone" "Copy" "Debug"]}}
   nm
   (mapv field fields)])

(defn -main [path]
  (let [json (read-json path)
        decls (mapv struct-decl json)]
    (println do-not-edit)
    (println (slurp (io/resource "preamble/frame/transpose.rs")))
    (println)
    (doseq [decl decls]
      (println (emit-expr decl) "\n"))))
