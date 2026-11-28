<script lang="ts">
  import {
    Box3,
    DirectionalLight,
    Group,
    HemisphereLight,
    Mesh,
    MeshLambertMaterial,
    PerspectiveCamera,
    Scene,
    Vector3,
    WebGLRenderer
  } from 'three';
  import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
  import { STLLoader } from 'three/addons/loaders/STLLoader.js';
  import type { PlateReport } from '../engine/client';
  import Segmented from './Segmented.svelte';
  import { text } from './text';
  import type { PrintMeshes, Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    plate: PlateReport;
    width: number;
    height: number;
    dark: boolean;
  }

  let { bench, plate, width, height, dark }: Props = $props();
  const instructionsId = $props.id();
  let canvas = $state<HTMLCanvasElement>();
  let parts = $state.raw<{ plate: PlateReport; meshes: PrintMeshes }>();
  let rig = $state.raw<ReturnType<typeof viewer>>();
  let failed = $state(false);
  let position = $state<'seated' | 'exploded'>('seated');
  const geometryCurrent = $derived(
    bench.current && bench.valid && !bench.opening && parts?.plate === plate
  );
  const loading = $derived(!geometryCurrent && (!bench.current || bench.opening || bench.valid));
  const showFrame = $derived(geometryCurrent && !!parts?.meshes.frame);
  const poses = (['seated', 'exploded'] as const).map((value) => ({
    value,
    label: text.registration.poses[value]
  }));

  $effect(() => {
    if (!canvas) return;
    try {
      const scene = viewer(canvas, () => (failed = true));
      rig = scene;
      return () => {
        scene.dispose();
      };
    } catch {
      failed = true;
    }
  });

  $effect(() => {
    if (!bench.ready || geometryCurrent || failed) return;
    const requested = plate;
    let waiting = true;
    void bench.printMeshes(requested).then(
      (next) => {
        if (waiting && next) parts = { plate: requested, meshes: next };
      },
      () => {
        if (waiting) failed = true;
      }
    );
    return () => {
      waiting = false;
    };
  });

  $effect(() => {
    if (parts && rig) {
      try {
        return rig.mount(parts.meshes);
      } catch {
        failed = true;
      }
    }
  });

  $effect(() => {
    if (parts) rig?.show(position === 'exploded', dark);
  });

  function viewer(canvas: HTMLCanvasElement, lost: () => void) {
    const scene = new Scene();
    const renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    const camera = new PerspectiveCamera(36, 1, 0.1, 10000);
    camera.up.set(0, 0, 1);
    camera.position.set(200, -260, 200);
    const orbit = new OrbitControls(camera, canvas);
    const light = new DirectionalLight(0xffffff, 2);
    light.position.set(-4, -5, 9);
    scene.add(light, new HemisphereLight(0xf4f7ff, 0x535964, 2));

    const group = new Group();
    scene.add(group);
    const loader = new STLLoader();
    const plateMaterial = new MeshLambertMaterial();
    const frameMaterial = new MeshLambertMaterial();
    let top: Mesh | undefined;
    let frame: Mesh | undefined;
    let extent = 0;
    let lift = 0;
    const centre = new Vector3();

    const render = (): void => {
      renderer.render(scene, camera);
    };
    const reset = (): void => {
      if (!extent) return;
      const fit = Math.min(1, camera.aspect);
      // Leave room above the frame for the plate to lift.
      const distance = (extent / 2 + lift) / (Math.sin((camera.fov * Math.PI) / 360) * fit);
      orbit.target.copy(centre);
      camera.position
        .copy(orbit.target)
        .addScaledVector(new Vector3(0.9, -1.5, 1.2).normalize(), distance);
      camera.near = distance / 400;
      camera.far = distance * 20;
      camera.updateProjectionMatrix();
      orbit.minDistance = extent / 4;
      orbit.maxDistance = distance * 3;
      orbit.update();
      render();
    };
    const size = (): void => {
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      if (w <= 0 || h <= 0) return;
      renderer.setSize(w, h, false);
      const previous = camera.aspect;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      if (previous !== camera.aspect && extent) reset();
      else render();
    };
    const resize = new ResizeObserver(size);
    resize.observe(canvas);
    orbit.addEventListener('change', render);

    const arrows = new Map([
      ['ArrowLeft', [-1, 0]],
      ['ArrowRight', [1, 0]],
      ['ArrowUp', [0, -1]],
      ['ArrowDown', [0, 1]]
    ]);
    const zoomStep = 1 / 1.2;
    const key = (event: KeyboardEvent): void => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const arrow = arrows.get(event.key);
      if (arrow) {
        const [x = 0, y = 0] = arrow;
        if (event.shiftKey) orbit.pan(x * 20, y * 20);
        else if (x) orbit.rotateLeft(-x * 0.13);
        else orbit.rotateUp(-y * 0.13);
      } else {
        switch (event.key) {
          case '+':
          case '=':
            orbit.dollyIn(zoomStep);
            break;
          case '-':
            orbit.dollyOut(zoomStep);
            break;
          case '0':
            reset();
            break;
          default:
            return;
        }
      }
      event.preventDefault();
    };
    canvas.addEventListener('webglcontextlost', lost);

    function clear(): void {
      top?.geometry.dispose();
      frame?.geometry.dispose();
      group.clear();
      top = undefined;
      frame = undefined;
    }
    return {
      mount(parts: PrintMeshes): () => void {
        try {
          top = new Mesh(loader.parse(parts.plate.slice().buffer), plateMaterial);
          top.position.set(parts.insetMm, parts.insetMm, 0);
          group.add(top);
          if (parts.frame) {
            frame = new Mesh(loader.parse(parts.frame.slice().buffer), frameMaterial);
            group.add(frame);
          }
          const box = new Box3().setFromObject(group);
          extent = box.getSize(new Vector3()).length();
          lift = parts.frame ? Math.max(8, extent * 0.12) : 0;
          box.getCenter(centre);
          centre.z += lift * 0.25;
          reset();
          return clear;
        } catch (error) {
          clear();
          throw error;
        }
      },
      show(exploded: boolean, dark: boolean): void {
        plateMaterial.color.set(dark ? 0x8ba6c1 : 0x90a6c0);
        frameMaterial.color.set(dark ? 0x555b68 : 0x555962);
        if (top) top.position.z = exploded ? lift : 0;
        render();
      },
      reset,
      key,
      dispose(): void {
        clear();
        resize.disconnect();
        canvas.removeEventListener('webglcontextlost', lost);
        orbit.dispose();
        plateMaterial.dispose();
        frameMaterial.dispose();
        renderer.dispose();
        renderer.forceContextLoss();
      }
    };
  }
</script>

<figure style:width="{width}px">
  <figcaption class="label">
    {text.freeform.plateFor(plate.paint, plate.colour)}
    {#if bench.freeform?.frame}
      · {text.registration.heading}{/if}
  </figcaption>
  {#if failed}
    <p class="notice">{text.printPreview.unavailable}</p>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (The 3D viewport is an application with arrow-key rotation.) -->
    <div
      class="space"
      aria-busy={loading}
      style:height="{height}px"
      role="application"
      tabindex="0"
      aria-label={text.printPreview.label}
      aria-describedby={instructionsId}
      onkeydown={(event) => {
        rig?.key(event);
      }}
    >
      <canvas class:faded={!geometryCurrent} bind:this={canvas}></canvas>
      {#if loading}<p class="loading note" role="status">
          {text.printPreview.loading}
        </p>{/if}
    </div>
    <div class="controls">
      {#if showFrame}
        <Segmented
          label={text.registration.assembly}
          options={poses}
          value={position}
          onpick={(value: 'seated' | 'exploded') => {
            position = value;
          }}
        />
      {/if}
      <button
        class="button"
        type="button"
        disabled={!geometryCurrent}
        onclick={() => {
          rig?.reset();
        }}
      >
        {text.printPreview.reset}
      </button>
    </div>
    <p id={instructionsId} class="note">{text.printPreview.interaction}</p>
    {#if showFrame}<p class="note">{text.registration.lift}</p>{/if}
  {/if}
</figure>

<style>
  figure {
    display: grid;
    gap: 8px;
    margin: 0;
    max-width: 100%;
    min-width: 0;
  }

  .space {
    position: relative;
    border: 1px solid var(--line);
    background: var(--surface);
  }

  .faded {
    opacity: 0.4;
  }

  .loading {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
  }

  canvas:active {
    cursor: grabbing;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .button {
    min-height: 34px;
    padding-inline: 8px;
    font-size: 11px;
  }
</style>
