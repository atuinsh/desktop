import { useCallback, useRef, useState, useEffect } from "react";

export type UseReducerWithEffectsReducerReturn<S, E> = S | [S, E[]];

/**
 * A hook that combines a reducer with a way to run declarative effects after the state is updated.
 * 
 * The reducer can return a single state value, or a tuple containing the new state and an array of effects.
 * Each effect is run in turn by the passed effect runner.
 * 
 * @param reducer - The reducer function that takes the current state and an action and returns a new state or a tuple containing the new state and an array of effects.
 * @param initialState - The initial state of the reducer.
 * @param effectRunner - The function that runs the effects after the state is updated.
 * @returns A tuple containing the current state and a dispatch function that can be used to dispatch actions to the reducer.
 */
export default function useReducerWithEffects<S, A, E>(
  reducer: (state: S, action: A) => UseReducerWithEffectsReducerReturn<S, E>,
  initialState: S,
  effectRunner: (effect: E, dispatch: (action: A) => void) => void | Promise<void>
): [S, (action: A) => void] {
  const [state, setState] = useState(initialState);
  const pendingEffectsRef = useRef<E[]>([]);
  const dispatchRef = useRef<(action: A) => void>(() => { });

  useEffect(() => {
    const effects = pendingEffectsRef.current;
    pendingEffectsRef.current = [];
    effects.forEach((effect) => effectRunner(effect, dispatchRef.current));
  })

  const dispatch = useCallback((action: A) => {
    setState((currentState) => {
      const result = reducer(currentState, action);
      if (Array.isArray(result)) {
        let [newState, effects] = result;
        if (!Array.isArray(effects)) {
          effects = [effects];
        }
        pendingEffectsRef.current = effects;
        return newState;
      }
      return result;
    })
  }, [reducer]);
  dispatchRef.current = dispatch;

  return [state, dispatch];
}
