# Modelos da IA

Os arquivos ficam no repositório para o build funcionar sem internet.

## Detector de objetos

`efficientdet_lite0.tflite`: MediaPipe Object Detector, EfficientDet-Lite0 (int8, entrada 320×320, 80 classes do COCO), versão 1.

- Origem: <https://storage.googleapis.com/mediapipe-models/object_detector/efficientdet_lite0/int8/1/efficientdet_lite0.tflite>
- SHA-256: `0720bf247bd76e6594ea28fa9c6f7c5242be774818997dbbeffc4da460c723bb`
- Licença: Apache-2.0 (EfficientDet, [google/automl](https://github.com/google/automl); ver `../LICENSE-mediapipe.txt`)
- Documentação: <https://ai.google.dev/edge/mediapipe/solutions/vision/object_detector>

Para conferir:

```sh
sha256sum ui/src/ai/models/*.tflite
```
