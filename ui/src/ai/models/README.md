# Modelos da IA

Os arquivos ficam no repositório para o build funcionar sem internet.

## Detector de objetos

`efficientdet_lite0.tflite`: MediaPipe Object Detector, EfficientDet-Lite0 (int8, entrada 320×320, 80 classes do COCO), versão 1.

- Origem: <https://storage.googleapis.com/mediapipe-models/object_detector/efficientdet_lite0/int8/1/efficientdet_lite0.tflite>
- SHA-256: `0720bf247bd76e6594ea28fa9c6f7c5242be774818997dbbeffc4da460c723bb`
- Licença: Apache-2.0 (EfficientDet, [google/automl](https://github.com/google/automl); ver `../LICENSE-mediapipe.txt`)
- Documentação: <https://ai.google.dev/edge/mediapipe/solutions/vision/object_detector>

## Reconhecedor de gestos

`gesture_recognizer.task`: MediaPipe Gesture Recognizer (float16, versão 1). É um pacote com o detector de mãos e os 21 pontos da mão (`hand_landmarker.task`) e o classificador dos gestos (`hand_gesture_recognizer.task`). O app usa três gestos: `Victory`, `Open_Palm` e `Closed_Fist`.

- Origem: <https://storage.googleapis.com/mediapipe-models/gesture_recognizer/gesture_recognizer/float16/1/gesture_recognizer.task>
- SHA-256: `97952348cf6a6a4915c2ea1496b4b37ebabc50cbbf80571435643c455f2b0482`
- Licença: Apache-2.0 (ver `../LICENSE-mediapipe.txt`)
- Documentação: <https://ai.google.dev/edge/mediapipe/solutions/vision/gesture_recognizer>

Para conferir:

```sh
sha256sum ui/src/ai/models/*.tflite ui/src/ai/models/*.task
```
