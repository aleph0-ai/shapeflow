/-!
Proof scope:
- Theorem-level model for text semantic invariance under bounded alteration profiles.
- Decode(Alter(scene)) = scene equivalence over event/pair semantic structures.

Runtime refinement targets:
- crates/shapeflow-core/src/text_semantics.rs
- crates/shapeflow-core/src/text_encoding.rs
-/

namespace ShapeFlow

inductive HorizontalRel where
  | leftOf
  | rightOf
  | alignedHorizontally
  deriving DecidableEq, Repr

inductive VerticalRel where
  | above
  | below
  | alignedVertically
  deriving DecidableEq, Repr

structure EventSem where
  eventIndex : Nat
  shapeId : String
  startX : Rat
  startY : Rat
  endX : Rat
  endY : Rat
  durationFrames : Nat
  easingToken : String
  simultaneousWith : List String
  deriving DecidableEq, Repr

structure PairSem where
  pairIndex : Nat
  eventIndex : Nat
  firstShapeId : String
  secondShapeId : String
  horizontalRel : HorizontalRel
  verticalRel : VerticalRel
  deriving DecidableEq, Repr

structure SceneSem where
  sceneIndex : Nat
  events : List EventSem
  pairs : List PairSem
  deriving DecidableEq, Repr

inductive AlterationProfile where
  | canonical
  | eventClauseReordered
  | pairClauseReordered
  | fullyReordered
  deriving DecidableEq, Repr

structure EventSurface where
  semantic : EventSem
  orderTag : Nat
  deriving DecidableEq, Repr

structure PairSurface where
  eventIndex : Nat
  subject : String
  object : String
  horizontalRel : HorizontalRel
  verticalRel : VerticalRel
  syntaxTag : Nat
  pairIndex : Nat
  deriving DecidableEq, Repr

structure SceneSurface where
  sceneIndex : Nat
  events : List EventSurface
  pairs : List PairSurface
  deriving DecidableEq, Repr

def alterEvent (profile : AlterationProfile) (event : EventSem) : EventSurface :=
  match profile with
  | .canonical => { semantic := event, orderTag := 0 }
  | .eventClauseReordered => { semantic := event, orderTag := 1 }
  | .pairClauseReordered => { semantic := event, orderTag := 0 }
  | .fullyReordered => { semantic := event, orderTag := 1 }

def decodeEvent (surface : EventSurface) : EventSem :=
  surface.semantic

def alterPair (profile : AlterationProfile) (pair : PairSem) : PairSurface :=
  match profile with
  | .canonical
  | .eventClauseReordered =>
      {
        subject := pair.firstShapeId
        object := pair.secondShapeId
        eventIndex := pair.eventIndex
        horizontalRel := pair.horizontalRel
        verticalRel := pair.verticalRel
        syntaxTag := 0
        pairIndex := pair.pairIndex
      }
  | .pairClauseReordered
  | .fullyReordered =>
      {
        subject := pair.firstShapeId
        object := pair.secondShapeId
        eventIndex := pair.eventIndex
        horizontalRel := pair.horizontalRel
        verticalRel := pair.verticalRel
        syntaxTag := 1
        pairIndex := pair.pairIndex
      }

def decodePair (surface : PairSurface) : PairSem :=
  {
    pairIndex := surface.pairIndex
    eventIndex := surface.eventIndex
    firstShapeId := surface.subject
    secondShapeId := surface.object
    horizontalRel := surface.horizontalRel
    verticalRel := surface.verticalRel
  }

def alterScene (profile : AlterationProfile) (scene : SceneSem) : SceneSurface :=
  {
    sceneIndex := scene.sceneIndex
    events := scene.events.map (alterEvent profile)
    pairs := scene.pairs.map (alterPair profile)
  }

def decodeScene (surface : SceneSurface) : SceneSem :=
  {
    sceneIndex := surface.sceneIndex
    events := surface.events.map decodeEvent
    pairs := surface.pairs.map decodePair
  }

theorem decode_alterEvent_eq (profile : AlterationProfile) (event : EventSem) :
    decodeEvent (alterEvent profile event) = event := by
  cases profile <;> rfl

theorem decode_alterPair_eq (profile : AlterationProfile) (pair : PairSem) :
    decodePair (alterPair profile pair) = pair := by
  cases pair with
  | mk pairIndex eventIndex firstShapeId secondShapeId horizontalRel verticalRel =>
      cases profile <;> rfl

theorem decode_alterScene_eq (profile : AlterationProfile) (scene : SceneSem) :
    decodeScene (alterScene profile scene) = scene := by
  cases scene with
  | mk sceneIndex events pairs =>
      simp [decodeScene, alterScene]
      constructor
      · induction events with
        | nil => rfl
        | cons event tail ih =>
            simp [decode_alterEvent_eq, ih]
      · induction pairs with
        | nil => rfl
        | cons pair tail ih =>
            simp [decode_alterPair_eq, ih]

end ShapeFlow
