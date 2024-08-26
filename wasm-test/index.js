import init, { SchematicWrapper } from './minecraft_schematic_utils.js';

let schematic;

async function initWasm() {
    await init();
    schematic = new SchematicWrapper();
    console.log("Wasm module initialized");
}

initWasm();

document.getElementById('convertButton').addEventListener('click', async () => {
    const fileInput = document.getElementById('fileInput');
    const file = fileInput.files[0];
    if (!file) {
        alert('Please select a file');
        return;
    }

    const arrayBuffer = await file.arrayBuffer();
    const uint8Array = new Uint8Array(arrayBuffer);

    console.log(`File loaded: ${file.name}, size: ${uint8Array.length} bytes`);

    try {
        if (file.name.endsWith('.litematic')) {
            console.log("Processing .litematic file");
            await schematic.from_litematic(uint8Array);
            console.log("Litematic processed, converting to schematic");
            const schemData = await schematic.to_schematic();
            console.log(`Conversion completed, schematic size: ${schemData.length} bytes`);
            downloadBlob(schemData, 'converted.schem', 'application/octet-stream');
        } else if (file.name.endsWith('.schem')) {
            console.log("Processing .schem file");
            await schematic.from_schematic(uint8Array);
            console.log("Schematic processed, converting to litematic");
            const litematicData = await schematic.to_litematic();
            console.log(`Conversion completed, litematic size: ${litematicData.length} bytes`);
            downloadBlob(litematicData, 'converted.litematic', 'application/octet-stream');
        } else {
            alert('Unsupported file format');
        }
    } catch (error) {
        console.error('Conversion error:', error);
        alert('Error during conversion: ' + error.message);
    }
});

document.getElementById('printButton').addEventListener('click', () => {
    if (!schematic) {
        alert('Please load a schematic first');
        return;
    }
    console.log("Printing schematic");
    const output = schematic.print_schematic();
    console.log("Schematic printed:", output);
    document.getElementById('output').textContent = output;
});

document.getElementById('debugButton').addEventListener('click', () => {
    if (!schematic) {
        alert('Please load a schematic first');
        return;
    }
    console.log("Debugging schematic");
    const debugInfo = debug_schematic(schematic);
    console.log("Debug info:", debugInfo);
    document.getElementById('output').textContent = debugInfo;
});

function downloadBlob(data, fileName, mimeType) {
    const blob = new Blob([data], { type: mimeType });
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.style.display = 'none';
    a.href = url;
    a.download = fileName;
    document.body.appendChild(a);
    a.click();
    window.URL.revokeObjectURL(url);
}