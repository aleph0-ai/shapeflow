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
  subject : String
  object : String
  horizontalRel : HorizontalRel
  verticalRel : VerticalRel
  swapped : Bool
  pairIndex : Nat
  deriving DecidableEq, Repr

structure SceneSurface where
  sceneIndex : Nat
  events : List EventSurface
  pairs : List PairSurface
  deriving DecidableEq, Repr

def flipHorizontal : HorizontalRel → HorizontalRel
  | .leftOf => .rightOf
  | .rightOf => .leftOf
  | .alignedHorizontally => .alignedHorizontally

def flipVertical : VerticalRel → VerticalRel
  | .above => .below
  | .below => .above
  | .alignedVertically => .alignedVertically

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
        horizontalRel := pair.horizontalRel
        verticalRel := pair.verticalRel
        swapped := false
        pairIndex := pair.pairIndex
      }
  | .pairClauseReordered
  | .fullyReordered =>
      {
        subject := pair.secondShapeId
        object := pair.firstShapeId
        horizontalRel := flipHorizontal pair.horizontalRel
        verticalRel := flipVertical pair.verticalRel
        swapped := true
        pairIndex := pair.pairIndex
      }

def decodePair (surface : PairSurface) : PairSem :=
  if surface.swapped then
    {
      pairIndex := surface.pairIndex
      firstShapeId := surface.object
      secondShapeId := surface.subject
      horizontalRel := flipHorizontal surface.horizontalRel
      verticalRel := flipVertical surface.verticalRel
    }
  else
    {
      pairIndex := surface.pairIndex
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

theorem flipHorizontal_involutive (rel : HorizontalRel) :
    flipHorizontal (flipHorizontal rel) = rel := by
  cases rel <;> rfl

theorem flipVertical_involutive (rel : VerticalRel) :
    flipVertical (flipVertical rel) = rel := by
  cases rel <;> rfl

theorem decode_alterEvent_eq (profile : AlterationProfile) (event : EventSem) :
    decodeEvent (alterEvent profile event) = event := by
  cases profile <;> rfl

theorem decode_alterPair_eq (profile : AlterationProfile) (pair : PairSem) :
    decodePair (alterPair profile pair) = pair := by
  cases pair with
  | mk pairIndex firstShapeId secondShapeId horizontalRel verticalRel =>
      cases profile <;> cases horizontalRel <;> cases verticalRel <;> rfl

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
