use godot::builtin::math::ApproxEq;
use godot::{
    builtin::meta::{ConvertError, GodotConvert},
    prelude::*,
};
use serde::{Deserialize, Serialize};

// This input is populated by gdscript code that uses the input map in
// the godot project settings. Ideally they should match each other
// Inputs may also include events with higher level data
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct Input {
    pub movement: Vector2,
    pub look: Vector2,
    pub jump: bool,
    pub punch: bool,
    pub events: Vec<InputEvent>,
}

impl ApproxEq for Input {
    fn approx_eq(&self, other: &Self) -> bool {
        self.movement.approx_eq(&other.movement)
            && self.look.approx_eq(&other.look)
            && self.jump.eq(&other.jump)
            && self.punch.eq(&other.punch)
            && self.events.len() == other.events.len()
            && self
                .events
                .iter()
                .zip(other.events.iter())
                .all(|(a, b)| a.approx_eq(b))
    }
}

impl GodotConvert for Input {
    type Via = Dictionary;
}

impl ToGodot for Input {
    fn to_godot(&self) -> Self::Via {
        let mut dictionary = Dictionary::new();
        dictionary.insert("movement", self.movement.clone());
        dictionary.insert("look", self.look.clone());
        dictionary.insert("jump", self.jump);
        dictionary.insert("punch", self.punch);
        let events_array = Array::from_iter(self.events.iter().map(|e| e.to_godot()));
        dictionary.insert("events", events_array);
        dictionary
    }
}

impl FromGodot for Input {
    fn try_from_godot(via: Self::Via) -> Result<Self, ConvertError> {
        let movement = via
            .get("movement")
            .ok_or(ConvertError::new("Missing movement"))?
            .try_to::<Vector2>()?;
        let look = via
            .get("look")
            .ok_or(ConvertError::new("Misisng look"))?
            .try_to::<Vector2>()?;
        let jump = via
            .get("jump")
            .ok_or(ConvertError::new("Missing jump"))?
            .try_to::<bool>()?;
        let punch = via
            .get("punch")
            .ok_or(ConvertError::new("Missing punch"))?
            .try_to::<bool>()?;
        let mut events = Vec::new();
        let events_array = via
            .get("events")
            .ok_or(ConvertError::new("Missing events"))?
            .try_to::<Array<Variant>>()?;
        for event in events_array.iter_shared() {
            let event = event.try_to::<Dictionary>()?;
            events.push(InputEvent::try_from_godot(event)?);
        }
        Ok(Self {
            movement,
            look,
            jump,
            punch,
            events,
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum InputEvent {
    SpawnBall {
        position: Vector3,
        direction: Vector3,
    },
}

impl InputEvent {
    pub fn kind(&self) -> &str {
        match self {
            InputEvent::SpawnBall { .. } => "spawn_ball",
        }
    }
}

impl ApproxEq for InputEvent {
    fn approx_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                InputEvent::SpawnBall {
                    position: position1,
                    direction: direction1,
                },
                InputEvent::SpawnBall {
                    position: position2,
                    direction: direction2,
                },
            ) => position1.approx_eq(position2) && direction1.approx_eq(direction2),
        }
    }
}

impl Default for InputEvent {
    fn default() -> Self {
        InputEvent::SpawnBall {
            position: Vector3::default(),
            direction: Vector3::default(),
        }
    }
}

impl GodotConvert for InputEvent {
    type Via = Dictionary;
}

impl ToGodot for InputEvent {
    fn to_godot(&self) -> Self::Via {
        let mut dictionary = Dictionary::new();
        match self {
            InputEvent::SpawnBall {
                position,
                direction,
            } => {
                dictionary.insert("type", "spawn_ball");
                dictionary.insert("position", position.clone());
                dictionary.insert("direction", direction.clone());
            }
        }
        dictionary
    }
}

impl FromGodot for InputEvent {
    fn try_from_godot(via: Self::Via) -> Result<Self, ConvertError> {
        let event_type = via
            .get("type")
            .ok_or(Default::default())?
            .try_to::<String>()?;
        match event_type.as_str() {
            "spawn_ball" => {
                let position = via
                    .get("position")
                    .ok_or(Default::default())?
                    .try_to::<Vector3>()?;
                let direction = via
                    .get("direction")
                    .ok_or(Default::default())?
                    .try_to::<Vector3>()?;
                Ok(InputEvent::SpawnBall {
                    position,
                    direction,
                })
            }
            _ => Err(Default::default()),
        }
    }
}
