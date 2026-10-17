(function () {
  //#region src/sandbox-worker.ts?worker&inline
  var jsContent =
    '(function() {\n	//#region src/capabilities.ts\n	const DENIED_WORKER_CAPABILITIES = [\n		"Date",\n		"fetch",\n		"WebSocket",\n		"EventSource",\n		"XMLHttpRequest",\n		"importScripts",\n		"indexedDB",\n		"caches",\n		"crypto",\n		"navigator",\n		"eval",\n		"Function"\n	];\n	/** Removes ambient capabilities before generated cartridge code is evaluated. */\n	function lockDownWorkerGlobals(target) {\n		for (const capability of DENIED_WORKER_CAPABILITIES) try {\n			Object.defineProperty(target, capability, {\n				configurable: false,\n				enumerable: false,\n				value: void 0,\n				writable: false\n			});\n		} catch {}\n		const deterministicMath = Object.freeze({\n			abs: Math.abs,\n			ceil: Math.ceil,\n			floor: Math.floor,\n			max: Math.max,\n			min: Math.min,\n			round: Math.round,\n			sign: Math.sign,\n			trunc: Math.trunc\n		});\n		try {\n			Object.defineProperty(target, "Math", {\n				configurable: false,\n				enumerable: false,\n				value: deterministicMath,\n				writable: false\n			});\n		} catch {}\n	}\n	//#endregion\n	//#region src/errors.ts\n	var RuntimeFault = class extends Error {\n		code;\n		sourceSpan;\n		constructor(code, message, sourceSpan) {\n			super(message);\n			this.name = "RuntimeFault";\n			this.code = code;\n			this.sourceSpan = sourceSpan;\n		}\n	};\n	var BudgetExceeded = class extends RuntimeFault {\n		used;\n		limit;\n		constructor(used, limit, sourceSpan) {\n			super("M019001", `frame used ${String(used)} synthetic work units; limit is ${String(limit)}`, sourceSpan);\n			this.name = "BudgetExceeded";\n			this.used = used;\n			this.limit = limit;\n		}\n	};\n	//#endregion\n	//#region src/hardware.ts\n	const HARDWARE = Object.freeze({\n		width: 240,\n		height: 144,\n		frameRate: 60,\n		paletteSize: 256,\n		rasterPaletteSize: 32,\n		transparentColor: 0,\n		visualCapacityBytes: 1048576,\n		saveCapacityBytes: 8192,\n		cartridgeCapacityBytes: 1048576,\n		drawCommandsPerFrame: 4096,\n		workUnitsPerFrame: 5e4,\n		audioVoices: 8,\n		audioSampleRate: 48e3,\n		trackerChannels: 8,\n		controllerPorts: 4,\n		spriteMaximumAxis: 128,\n		tileSize: 8\n	});\n	/** Hardware Revision 2 keeps the original 32 colours, then adds a 216-colour cube and eight greys. */\n	const MASTER_PALETTE = Object.freeze([\n		...[\n			"#17141f",\n			"#292532",\n			"#403946",\n			"#5d5054",\n			"#806a63",\n			"#aa8b74",\n			"#d5b992",\n			"#f4e5bd",\n			"#5b2938",\n			"#8b3c47",\n			"#bf5558",\n			"#ed7b69",\n			"#5a3928",\n			"#89572e",\n			"#c18436",\n			"#e7bd50",\n			"#263c32",\n			"#345f46",\n			"#4b8b58",\n			"#7fbd68",\n			"#203b47",\n			"#2e6571",\n			"#43969a",\n			"#75cbc0",\n			"#243451",\n			"#345581",\n			"#4b7db3",\n			"#73a9d1",\n			"#3e3154",\n			"#654777",\n			"#936397",\n			"#c38aae"\n		],\n		...Array.from({ length: 216 }, (_, index) => {\n			return `#${[\n				Math.floor(index / 36) * 51,\n				Math.floor(index / 6) % 6 * 51,\n				index % 6 * 51\n			].map((value) => value.toString(16).padStart(2, "0")).join("")}`;\n		}),\n		...Array.from({ length: 8 }, (_, index) => {\n			const value = (index * 29 + 26).toString(16).padStart(2, "0");\n			return `#${value}${value}${value}`;\n		})\n	]);\n	const MASTER_PALETTE_RGBA = Object.freeze(MASTER_PALETTE.flatMap((hex) => [\n		Number.parseInt(hex.slice(1, 3), 16),\n		Number.parseInt(hex.slice(3, 5), 16),\n		Number.parseInt(hex.slice(5, 7), 16),\n		255\n	]));\n	//#endregion\n	//#region src/budget.ts\n	function isWorkBudgetSnapshot(value) {\n		if (!isRecord$9(value) || Object.keys(value).length !== 4 || value.revision !== 1 || typeof value.limit !== "number" || !Number.isSafeInteger(value.limit) || value.limit < 1 || value.limit > HARDWARE.workUnitsPerFrame || typeof value.used !== "number" || !Number.isSafeInteger(value.used) || value.used < 0 || !Array.isArray(value.attribution) || value.attribution.length > HARDWARE.workUnitsPerFrame + 1) return false;\n		let total = 0;\n		let previousUnits = Infinity;\n		let previousStart = -1;\n		const spans = /* @__PURE__ */ new Set();\n		for (const entry of value.attribution) {\n			if (!isRecord$9(entry) || Object.keys(entry).length !== 2 || typeof entry.units !== "number" || !Number.isSafeInteger(entry.units) || entry.units < 0 || entry.units > value.used - total || !isRecord$9(entry.sourceSpan) || Object.keys(entry.sourceSpan).length !== 2 || typeof entry.sourceSpan.start !== "number" || !Number.isSafeInteger(entry.sourceSpan.start) || entry.sourceSpan.start < 0 || typeof entry.sourceSpan.end !== "number" || !Number.isSafeInteger(entry.sourceSpan.end) || entry.sourceSpan.end < entry.sourceSpan.start || entry.units > previousUnits || entry.units === previousUnits && entry.sourceSpan.start < previousStart) return false;\n			const key = `${String(entry.sourceSpan.start)}:${String(entry.sourceSpan.end)}`;\n			if (spans.has(key)) return false;\n			spans.add(key);\n			total += entry.units;\n			previousUnits = entry.units;\n			previousStart = entry.sourceSpan.start;\n		}\n		return total === value.used;\n	}\n	function isRecord$9(value) {\n		return typeof value === "object" && value !== null && !Array.isArray(value);\n	}\n	/** Per-frame synthetic execution budget and source-span attribution. */\n	var WorkBudget = class {\n		frameLimit;\n		usedUnits = 0;\n		bySpan = /* @__PURE__ */ new Map();\n		constructor(frameLimit) {\n			if (!Number.isSafeInteger(frameLimit) || frameLimit <= 0 || frameLimit > HARDWARE.workUnitsPerFrame) throw new RangeError("work-unit limit must be an integer between 1 and 50,000");\n			this.frameLimit = frameLimit;\n		}\n		get limit() {\n			return this.frameLimit;\n		}\n		get used() {\n			return this.usedUnits;\n		}\n		beginFrame() {\n			this.usedUnits = 0;\n			this.bySpan.clear();\n		}\n		snapshot() {\n			return structuredClone({\n				revision: 1,\n				limit: this.frameLimit,\n				used: this.usedUnits,\n				attribution: this.attribution()\n			});\n		}\n		restore(value) {\n			if (!isWorkBudgetSnapshot(value) || value.limit !== this.frameLimit) throw new TypeError("invalid or mismatched work-budget snapshot");\n			const entries = structuredClone(value.attribution);\n			this.usedUnits = value.used;\n			this.bySpan.clear();\n			for (const entry of entries) this.bySpan.set(`${String(entry.sourceSpan.start)}:${String(entry.sourceSpan.end)}`, entry);\n		}\n		charge(units, sourceSpan) {\n			if (!Number.isInteger(units) || units < 0) throw new RangeError("work-unit charge must be a non-negative finite integer");\n			const charged = Math.min(units, Number.MAX_SAFE_INTEGER - this.usedUnits);\n			this.usedUnits += charged;\n			const key = `${String(sourceSpan.start)}:${String(sourceSpan.end)}`;\n			const previous = this.bySpan.get(key);\n			this.bySpan.set(key, {\n				sourceSpan,\n				units: (previous?.units ?? 0) + charged\n			});\n			if (charged !== units || this.usedUnits > this.frameLimit) throw new BudgetExceeded(this.usedUnits, this.frameLimit, sourceSpan);\n		}\n		attribution() {\n			return [...this.bySpan.values()].sort((left, right) => right.units - left.units || left.sourceSpan.start - right.sourceSpan.start);\n		}\n	};\n	//#endregion\n	//#region src/font.ts\n	const BITMAP_FONT = Object.freeze({\n		glyphWidth: 5,\n		glyphHeight: 7,\n		advanceX: 6,\n		advanceY: 8\n	});\n	const GLYPHS = Object.freeze({\n		" ": [\n			0,\n			0,\n			0,\n			0,\n			0,\n			0,\n			0\n		],\n		"!": [\n			4,\n			4,\n			4,\n			4,\n			4,\n			0,\n			4\n		],\n		"\\"": [\n			10,\n			10,\n			10,\n			0,\n			0,\n			0,\n			0\n		],\n		"#": [\n			10,\n			31,\n			10,\n			10,\n			31,\n			10,\n			0\n		],\n		$: [\n			4,\n			15,\n			20,\n			14,\n			5,\n			30,\n			4\n		],\n		"%": [\n			25,\n			26,\n			4,\n			4,\n			11,\n			19,\n			0\n		],\n		"&": [\n			12,\n			18,\n			20,\n			8,\n			21,\n			18,\n			13\n		],\n		"\'": [\n			4,\n			4,\n			8,\n			0,\n			0,\n			0,\n			0\n		],\n		"(": [\n			2,\n			4,\n			8,\n			8,\n			8,\n			4,\n			2\n		],\n		")": [\n			8,\n			4,\n			2,\n			2,\n			2,\n			4,\n			8\n		],\n		"*": [\n			0,\n			21,\n			14,\n			31,\n			14,\n			21,\n			0\n		],\n		"+": [\n			0,\n			4,\n			4,\n			31,\n			4,\n			4,\n			0\n		],\n		",": [\n			0,\n			0,\n			0,\n			0,\n			4,\n			4,\n			8\n		],\n		"-": [\n			0,\n			0,\n			0,\n			31,\n			0,\n			0,\n			0\n		],\n		".": [\n			0,\n			0,\n			0,\n			0,\n			0,\n			12,\n			12\n		],\n		"/": [\n			1,\n			2,\n			2,\n			4,\n			8,\n			8,\n			16\n		],\n		"0": [\n			14,\n			17,\n			19,\n			21,\n			25,\n			17,\n			14\n		],\n		"1": [\n			4,\n			12,\n			4,\n			4,\n			4,\n			4,\n			14\n		],\n		"2": [\n			14,\n			17,\n			1,\n			2,\n			4,\n			8,\n			31\n		],\n		"3": [\n			30,\n			1,\n			1,\n			14,\n			1,\n			1,\n			30\n		],\n		"4": [\n			2,\n			6,\n			10,\n			18,\n			31,\n			2,\n			2\n		],\n		"5": [\n			31,\n			16,\n			16,\n			30,\n			1,\n			1,\n			30\n		],\n		"6": [\n			14,\n			16,\n			16,\n			30,\n			17,\n			17,\n			14\n		],\n		"7": [\n			31,\n			1,\n			2,\n			4,\n			8,\n			8,\n			8\n		],\n		"8": [\n			14,\n			17,\n			17,\n			14,\n			17,\n			17,\n			14\n		],\n		"9": [\n			14,\n			17,\n			17,\n			15,\n			1,\n			1,\n			14\n		],\n		":": [\n			0,\n			12,\n			12,\n			0,\n			12,\n			12,\n			0\n		],\n		";": [\n			0,\n			12,\n			12,\n			0,\n			4,\n			4,\n			8\n		],\n		"<": [\n			2,\n			4,\n			8,\n			16,\n			8,\n			4,\n			2\n		],\n		"=": [\n			0,\n			0,\n			31,\n			0,\n			31,\n			0,\n			0\n		],\n		">": [\n			8,\n			4,\n			2,\n			1,\n			2,\n			4,\n			8\n		],\n		"?": [\n			14,\n			17,\n			1,\n			2,\n			4,\n			0,\n			4\n		],\n		"@": [\n			14,\n			17,\n			23,\n			21,\n			23,\n			16,\n			14\n		],\n		A: [\n			14,\n			17,\n			17,\n			31,\n			17,\n			17,\n			17\n		],\n		B: [\n			30,\n			17,\n			17,\n			30,\n			17,\n			17,\n			30\n		],\n		C: [\n			14,\n			17,\n			16,\n			16,\n			16,\n			17,\n			14\n		],\n		D: [\n			28,\n			18,\n			17,\n			17,\n			17,\n			18,\n			28\n		],\n		E: [\n			31,\n			16,\n			16,\n			30,\n			16,\n			16,\n			31\n		],\n		F: [\n			31,\n			16,\n			16,\n			30,\n			16,\n			16,\n			16\n		],\n		G: [\n			14,\n			17,\n			16,\n			23,\n			17,\n			17,\n			15\n		],\n		H: [\n			17,\n			17,\n			17,\n			31,\n			17,\n			17,\n			17\n		],\n		I: [\n			14,\n			4,\n			4,\n			4,\n			4,\n			4,\n			14\n		],\n		J: [\n			7,\n			2,\n			2,\n			2,\n			2,\n			18,\n			12\n		],\n		K: [\n			17,\n			18,\n			20,\n			24,\n			20,\n			18,\n			17\n		],\n		L: [\n			16,\n			16,\n			16,\n			16,\n			16,\n			16,\n			31\n		],\n		M: [\n			17,\n			27,\n			21,\n			21,\n			17,\n			17,\n			17\n		],\n		N: [\n			17,\n			25,\n			25,\n			21,\n			19,\n			19,\n			17\n		],\n		O: [\n			14,\n			17,\n			17,\n			17,\n			17,\n			17,\n			14\n		],\n		P: [\n			30,\n			17,\n			17,\n			30,\n			16,\n			16,\n			16\n		],\n		Q: [\n			14,\n			17,\n			17,\n			17,\n			21,\n			18,\n			13\n		],\n		R: [\n			30,\n			17,\n			17,\n			30,\n			20,\n			18,\n			17\n		],\n		S: [\n			15,\n			16,\n			16,\n			14,\n			1,\n			1,\n			30\n		],\n		T: [\n			31,\n			4,\n			4,\n			4,\n			4,\n			4,\n			4\n		],\n		U: [\n			17,\n			17,\n			17,\n			17,\n			17,\n			17,\n			14\n		],\n		V: [\n			17,\n			17,\n			17,\n			17,\n			17,\n			10,\n			4\n		],\n		W: [\n			17,\n			17,\n			17,\n			21,\n			21,\n			21,\n			10\n		],\n		X: [\n			17,\n			17,\n			10,\n			4,\n			10,\n			17,\n			17\n		],\n		Y: [\n			17,\n			17,\n			10,\n			4,\n			4,\n			4,\n			4\n		],\n		Z: [\n			31,\n			1,\n			2,\n			4,\n			8,\n			16,\n			31\n		],\n		"[": [\n			14,\n			8,\n			8,\n			8,\n			8,\n			8,\n			14\n		],\n		"\\\\": [\n			16,\n			8,\n			8,\n			4,\n			2,\n			2,\n			1\n		],\n		"]": [\n			14,\n			2,\n			2,\n			2,\n			2,\n			2,\n			14\n		],\n		"^": [\n			4,\n			10,\n			17,\n			0,\n			0,\n			0,\n			0\n		],\n		_: [\n			0,\n			0,\n			0,\n			0,\n			0,\n			0,\n			31\n		],\n		"`": [\n			8,\n			4,\n			2,\n			0,\n			0,\n			0,\n			0\n		],\n		"{": [\n			3,\n			4,\n			4,\n			24,\n			4,\n			4,\n			3\n		],\n		"|": [\n			4,\n			4,\n			4,\n			4,\n			4,\n			4,\n			4\n		],\n		"}": [\n			24,\n			4,\n			4,\n			3,\n			4,\n			4,\n			24\n		],\n		"~": [\n			0,\n			0,\n			9,\n			22,\n			0,\n			0,\n			0\n		]\n	});\n	const FALLBACK = GLYPHS["?"];\n	/** Lowercase has a compact small-cap treatment in revision 1 of the original console font. */\n	function glyphRows(character) {\n		if (character.length !== 1) return FALLBACK;\n		return GLYPHS[character] ?? GLYPHS[character.toUpperCase()] ?? FALLBACK;\n	}\n	//#endregion\n	//#region src/bus.ts\n	const MEMORY = Object.freeze({\n		size: 4194304,\n		ram: 0,\n		ramBytes: 65536,\n		front: 65536,\n		back: 102400,\n		display: 139264,\n		visual: 196608,\n		visualExtended: 2162688,\n		draw: 327680,\n		transparency: 327760,\n		palette: 327808,\n		input: 327936,\n		inputBytes: 48,\n		system: 328192,\n		systemBytes: 64,\n		rasterLive: 328704,\n		visualInfo: 328448,\n		saveControl: 328960,\n		cartridgeInfo: 329216,\n		save: 360448,\n		saveCommitted: 368640,\n		cartridgeRom: 393216,\n		cartridgeRomExtension: 1048576,\n		paletteExtended: 2097152,\n		audio: 331776,\n		audioTracker: 331808,\n		audioVoices: 332032,\n		voiceStride: 64,\n		audioAssets: 3932160,\n		audioAssetStride: 32,\n		raster: 348160,\n		rasterStride: 40,\n		assets: 655360,\n		assetStride: 32,\n		allocations: 786432,\n		allocationStride: 24\n	});\n	function regionLength(region) {\n		return "bytes" in region ? region.bytes.length : region.length;\n	}\n	function isMemorySnapshot(value) {\n		if (typeof value !== "object" || value === null || !("revision" in value) || value.revision !== 1 || !("regions" in value) || !Array.isArray(value.regions) || Object.keys(value).length !== 2 || value.regions.length > 4096) return false;\n		let end = 0;\n		for (const region of value.regions) {\n			if (typeof region !== "object" || region === null || Object.keys(region).length !== 2 || !("address" in region) || typeof region.address !== "number" || !Number.isSafeInteger(region.address) || region.address < end || !("bytes" in region) || !(region.bytes instanceof Uint8Array) || region.bytes.length === 0 || region.bytes.length > MEMORY.size - region.address) return false;\n			end = region.address + region.bytes.length;\n		}\n		return true;\n	}\n	/** Byte-addressed access to real device storage; reserved holes read zero and reject writes. */\n	var MemoryBus = class {\n		regions;\n		charge;\n		constructor(regions, charge) {\n			this.regions = [...regions].sort((left, right) => left.address - right.address);\n			this.charge = charge;\n			let end = 0;\n			for (const region of this.regions) {\n				const length = regionLength(region);\n				if (!Number.isSafeInteger(region.address) || region.address < end || !Number.isSafeInteger(length) || length <= 0 || length > MEMORY.size - region.address) throw new TypeError("invalid or overlapping hardware region");\n				end = region.address + length;\n			}\n		}\n		read(address, width, span) {\n			this.range(address, width, span);\n			this.charge(width, span);\n			const low = this.byte(address);\n			return width === 1 ? low : low + this.byte(address + 1) * 256;\n		}\n		write(address, value, width, span, raster = false) {\n			this.range(address, width, span);\n			this.value(value, width === 1 ? 255 : 65535, span);\n			this.charge(width, span);\n			const bytes = width === 1 ? Uint8Array.of(value) : Uint8Array.of(value & 255, value >>> 8);\n			this.store(address, bytes, span, raster);\n		}\n		fill(address, value, length, span, raster = false) {\n			this.range(address, length, span);\n			this.value(value, 255, span);\n			this.charge(1 + length, span);\n			this.store(address, new Uint8Array(length).fill(value), span, raster);\n		}\n		copy(destination, source, length, span, raster = false) {\n			this.range(destination, length, span);\n			this.range(source, length, span);\n			this.charge(1 + 2 * length, span);\n			const bytes = new Uint8Array(length);\n			for (let index = 0; index < length; index += 1) bytes[index] = this.byte(source + index);\n			this.store(destination, bytes, span, raster);\n		}\n		/** Bounded debugger read at an idle Worker message boundary; does not charge cartridge work. */\n		inspect(address, length) {\n			this.range(address, length, {\n				start: 0,\n				end: 0\n			});\n			const bytes = new Uint8Array(length);\n			for (let index = 0; index < length; index += 1) bytes[index] = this.byte(address + index);\n			return bytes;\n		}\n		/** Transactional debugger edit at an idle Worker message boundary; does not charge work. */\n		edit(address, bytes) {\n			const span = {\n				start: 0,\n				end: 0\n			};\n			this.range(address, bytes.length, span);\n			this.store(address, bytes.slice(), span, false, true);\n		}\n		describe() {\n			return this.regions.map((region) => ({\n				name: region.name,\n				address: region.address,\n				length: regionLength(region),\n				writable: region.writable\n			}));\n		}\n		snapshot() {\n			return {\n				revision: 1,\n				regions: this.retained().map(({ address, bytes }) => ({\n					address,\n					bytes: bytes.slice()\n				}))\n			};\n		}\n		restore(snapshot) {\n			const retained = this.retained();\n			if (!isMemorySnapshot(snapshot) || snapshot.regions.length !== retained.length) throw new TypeError("invalid hardware memory snapshot");\n			for (let index = 0; index < retained.length; index += 1) {\n				const target = retained[index];\n				const source = snapshot.regions[index];\n				if (target === void 0 || source === void 0 || target.address !== source.address || target.bytes.length !== source.bytes.length || target.validate?.(0, source.bytes) === false) throw new TypeError("hardware memory snapshot does not match this cartridge");\n			}\n			for (let index = 0; index < retained.length; index += 1) retained[index]?.bytes.set(snapshot.regions[index]?.bytes ?? []);\n		}\n		retained() {\n			return this.regions.filter((region) => "bytes" in region && (region.retained ?? region.writable));\n		}\n		byte(address) {\n			const region = this.regions.find((entry) => address >= entry.address && address < entry.address + regionLength(entry));\n			if (region === void 0) return 0;\n			return "bytes" in region ? region.bytes[address - region.address] ?? 0 : region.readByte(address - region.address);\n		}\n		store(address, bytes, span, raster, debugEdit = false) {\n			const writes = [];\n			for (let index = 0; index < bytes.length;) {\n				const cursor = address + index;\n				const region = this.regions.find((entry) => cursor >= entry.address && cursor < entry.address + regionLength(entry));\n				if (region === void 0 || !region.writable) throw new RuntimeFault("M019021", "write to read-only or reserved hardware memory", span);\n				if (raster && !region.rasterWritable) throw new RuntimeFault("M019011", "hardware write is not valid in the raster callback", span);\n				const offset = cursor - region.address;\n				const count = Math.min(bytes.length - index, regionLength(region) - offset);\n				const part = bytes.subarray(index, index + count);\n				const commit = "bytes" in region ? region.validate?.(offset, part) === false ? void 0 : () => {\n					region.bytes.set(part, offset);\n				} : region.prepareWrite(offset, part, span, debugEdit);\n				if (commit === void 0) throw new RuntimeFault("M019022", `invalid value for hardware region \'${region.name}\'`, span);\n				writes.push(commit);\n				index += count;\n			}\n			for (const write of writes) write();\n		}\n		range(address, length, span) {\n			if (!Number.isSafeInteger(address) || !Number.isSafeInteger(length) || address < 0 || length < 0 || address > MEMORY.size || length > MEMORY.size - address) throw new RuntimeFault("M019020", "hardware address or range is out of bounds", span);\n		}\n		value(value, maximum, span) {\n			if (!Number.isSafeInteger(value) || value < 0 || value > maximum) throw new RuntimeFault("M019022", "hardware value is outside the unsigned operation width", span);\n		}\n	};\n	//#endregion\n	//#region src/visual-store.ts\n	const LEGACY_VISUAL_BYTES = 131072;\n	/** A single packed, little-endian visual image; all drawing views alias these bytes. */\n	var VisualAssetStore = class {\n		entries = /* @__PURE__ */ new Map();\n		ids = /* @__PURE__ */ new Map();\n		bytes = new Uint8Array(HARDWARE.visualCapacityBytes);\n		allocations = [];\n		descriptors;\n		allocationTable;\n		info = /* @__PURE__ */ new Uint8Array(24);\n		addressBase;\n		display;\n		usedBytes;\n		constructor(assets = [], display) {\n			if (assets.length > 4096) throw new RangeError("too many visual assets");\n			const sorted = [...assets].sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);\n			const source = /* @__PURE__ */ new Map();\n			let usedBytes = 0;\n			for (const [id, asset] of sorted.entries()) {\n				if (source.has(asset.name)) throw new TypeError(`duplicate visual asset \'${asset.name}\'`);\n				validateAsset(asset);\n				usedBytes += visualAssetBytes(asset);\n				source.set(asset.name, asset);\n				this.ids.set(asset.name, id);\n			}\n			if (display !== void 0) usedBytes += 32 + display.raster.length * 38;\n			if (usedBytes > HARDWARE.visualCapacityBytes) throw new RangeError("visual assets exceed the 1 MiB shared capacity");\n			this.usedBytes = usedBytes;\n			this.addressBase = usedBytes > LEGACY_VISUAL_BYTES ? MEMORY.visualExtended : MEMORY.visual;\n			this.descriptors = new Uint8Array(sorted.length * MEMORY.assetStride);\n			const descriptorView = new DataView(this.descriptors.buffer);\n			for (const [id, asset] of sorted.entries()) {\n				const first = this.allocations.length;\n				const stored = this.store(asset, source);\n				this.entries.set(asset.name, stored);\n				const offset = id * MEMORY.assetStride;\n				descriptorView.setUint32(offset, [\n					"sprite",\n					"animation",\n					"tile_set",\n					"map",\n					"font"\n				].indexOf(asset.kind) + 1, true);\n				descriptorView.setUint32(offset + 4, this.allocations.length - first, true);\n				descriptorView.setUint32(offset + 8, MEMORY.allocations + first * MEMORY.allocationStride, true);\n				descriptorView.setUint32(offset + 12, visualAssetBytes(asset), true);\n			}\n			this.display = display === void 0 ? void 0 : this.storeDisplay(display);\n			this.allocationTable = new Uint8Array(this.allocations.length * MEMORY.allocationStride);\n			const allocationView = new DataView(this.allocationTable.buffer);\n			for (const [index, entry] of this.allocations.entries()) {\n				const offset = index * MEMORY.allocationStride;\n				[\n					entry.kind,\n					this.addressBase + entry.offset,\n					entry.bytes.length,\n					entry.width,\n					entry.height,\n					entry.reference\n				].forEach((value, field) => {\n					allocationView.setUint32(offset + field * 4, value, true);\n				});\n			}\n			const info = new DataView(this.info.buffer);\n			[\n				sorted.length,\n				usedBytes,\n				MEMORY.assets,\n				this.allocations.length,\n				MEMORY.allocations,\n				display === void 0 ? 0 : this.addressBase + usedBytes - 32 - display.raster.length * 38\n			].forEach((value, field) => {\n				info.setUint32(field * 4, value, true);\n			});\n		}\n		get(name) {\n			return this.entries.get(name);\n		}\n		id(name) {\n			return this.ids.get(name) ?? -1;\n		}\n		mapCell(name, layer, x, y) {\n			const asset = this.entries.get(name);\n			if (asset?.kind !== "map") return void 0;\n			const selected = asset.layers[layer];\n			if (selected === void 0 || x < 0 || y < 0 || x >= selected.width || y >= selected.height) return void 0;\n			return selected.cells.getUint16((y * selected.width + x) * 2, true);\n		}\n		mapFlag(name, layer, x, y, flag) {\n			const asset = this.entries.get(name);\n			if (asset?.kind !== "map" || flag < 0 || flag > 7) return false;\n			const selected = asset.layers[layer];\n			if (selected === void 0) return false;\n			const tile = this.mapCell(name, layer, x, y);\n			const tileSet = this.entries.get(selected.tileSet);\n			return tile !== void 0 && tileSet?.kind === "tile_set" && ((tileSet.flags[tile] ?? 0) & 1 << flag) !== 0;\n		}\n		memoryRegions() {\n			return [\n				{\n					name: "visual store",\n					address: MEMORY.visual,\n					bytes: this.bytes.subarray(0, LEGACY_VISUAL_BYTES),\n					writable: true,\n					validate: (offset, bytes) => this.validate(offset, bytes)\n				},\n				...this.addressBase === MEMORY.visualExtended ? [{\n					name: "visual store extended",\n					address: MEMORY.visualExtended,\n					bytes: this.bytes,\n					writable: true,\n					validate: (offset, bytes) => this.validate(offset, bytes)\n				}] : [],\n				{\n					name: "visual allocation status",\n					address: MEMORY.visualInfo,\n					bytes: this.info,\n					writable: false\n				},\n				...this.descriptors.length === 0 ? [] : [{\n					name: "visual asset descriptors",\n					address: MEMORY.assets,\n					bytes: this.descriptors,\n					writable: false\n				}, {\n					name: "visual allocations",\n					address: MEMORY.allocations,\n					bytes: this.allocationTable,\n					writable: false\n				}],\n				...this.descriptors.length === 0 && this.allocationTable.length > 0 ? [{\n					name: "visual allocations",\n					address: MEMORY.allocations,\n					bytes: this.allocationTable,\n					writable: false\n				}] : []\n			];\n		}\n		allocate(kind, width, height, data, reference = 0, validate) {\n			const previous = this.allocations.at(-1);\n			const offset = previous === void 0 ? 0 : previous.offset + previous.bytes.length;\n			const bytes = this.bytes.subarray(offset, offset + data.length);\n			bytes.set(data);\n			this.allocations.push({\n				offset,\n				bytes,\n				kind,\n				width,\n				height,\n				reference,\n				...validate === void 0 ? {} : { validate }\n			});\n			return bytes;\n		}\n		sprite(sprite) {\n			return {\n				...sprite,\n				pixels: this.allocate(1, sprite.width, sprite.height, sprite.pixels, 0, (_offset, bytes) => bytes.every((color) => color < HARDWARE.paletteSize))\n			};\n		}\n		store(asset, source) {\n			switch (asset.kind) {\n				case "sprite": return this.sprite(asset);\n				case "animation": return {\n					...asset,\n					frames: asset.frames.map((frame) => this.sprite(frame))\n				};\n				case "tile_set": return {\n					...asset,\n					tiles: asset.tiles.map((tile) => this.sprite(tile)),\n					flags: this.allocate(3, asset.flags.length, 1, asset.flags)\n				};\n				case "map": return {\n					...asset,\n					layers: asset.layers.map((layer) => {\n						const tileSet = source.get(layer.tileSet);\n						if (tileSet?.kind !== "tile_set" || layer.cells.some((tile) => tile >= tileSet.tiles.length)) throw new TypeError(`map \'${asset.name}\' references an invalid tile set or tile`);\n						const encoded = new Uint8Array(layer.cells.length * 2);\n						const view = new DataView(encoded.buffer);\n						for (let index = 0; index < layer.cells.length; index += 1) view.setUint16(index * 2, layer.cells[index] ?? 0, true);\n						const bytes = this.allocate(2, layer.width, layer.height, encoded, (this.ids.get(layer.tileSet) ?? -1) + 1, (offset, part) => {\n							const byte = (index) => (index >= offset && index < offset + part.length ? part[index - offset] : bytes[index]) ?? 0;\n							for (let index = offset - offset % 2; index < offset + part.length; index += 2) if (byte(index) + byte(index + 1) * 256 >= tileSet.tiles.length) return false;\n							return true;\n						});\n						return {\n							...layer,\n							cells: new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)\n						};\n					})\n				};\n				case "font": {\n					const glyphs = [...asset.glyphs.entries()].sort(([left], [right]) => left - right);\n					const glyphBytes = asset.glyphWidth * asset.glyphHeight;\n					const encoded = new Uint8Array(8 + glyphs.length * (2 + glyphBytes));\n					encoded.set([\n						asset.glyphWidth,\n						asset.glyphHeight,\n						asset.baseline,\n						asset.advanceX,\n						asset.advanceY,\n						asset.missingGlyph\n					], 0);\n					new DataView(encoded.buffer).setUint16(6, glyphs.length, true);\n					const bitmapOffsets = /* @__PURE__ */ new Set();\n					for (const [index, [code, pixels]] of glyphs.entries()) {\n						const offset = 8 + index * (2 + glyphBytes);\n						new DataView(encoded.buffer).setUint16(offset, code, true);\n						encoded.set(pixels, offset + 2);\n						for (let byte = offset + 2; byte < offset + 2 + glyphBytes; byte += 1) bitmapOffsets.add(byte);\n					}\n					const bytes = this.allocate(6, asset.glyphWidth, asset.glyphHeight, encoded, asset.missingGlyph, (offset, part) => part.every((value, index) => {\n						const absolute = offset + index;\n						return bitmapOffsets.has(absolute) ? value <= 1 : value === encoded[absolute];\n					}));\n					return {\n						...asset,\n						glyphs: new Map(glyphs.map(([code], index) => {\n							const offset = 8 + index * (2 + glyphBytes) + 2;\n							return [code, bytes.subarray(offset, offset + glyphBytes)];\n						}))\n					};\n				}\n			}\n		}\n		storeDisplay(display) {\n			if (display.remap.length !== HARDWARE.rasterPaletteSize || display.raster.length > 144) throw new TypeError("invalid display defaults");\n			const remap = this.allocate(4, 32, 1, display.remap, 0, (_offset, bytes) => bytes.every((value) => value < HARDWARE.paletteSize));\n			let previous = -1;\n			return {\n				remap,\n				raster: display.raster.map((row) => {\n					if (!Number.isInteger(row.line) || row.line <= previous || row.line >= 144 || ![row.scrollX, row.scrollY].every((value) => Number.isInteger(value) && value >= -32768 && value <= 32767) || row.remap.length !== HARDWARE.rasterPaletteSize) throw new TypeError("invalid display raster defaults");\n					previous = row.line;\n					const encoded = /* @__PURE__ */ new Uint8Array(38);\n					const view = new DataView(encoded.buffer);\n					view.setUint16(0, row.line, true);\n					view.setInt16(2, row.scrollX, true);\n					view.setInt16(4, row.scrollY, true);\n					encoded.set(row.remap, 6);\n					const bytes = this.allocate(5, 38, 1, encoded, 0, (offset, part) => part.every((value, index) => offset + index < 2 ? value === encoded[offset + index] : offset + index < 6 || value < HARDWARE.paletteSize));\n					const stored = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);\n					return {\n						line: row.line,\n						get scrollX() {\n							return stored.getInt16(2, true);\n						},\n						get scrollY() {\n							return stored.getInt16(4, true);\n						},\n						remap: bytes.subarray(6)\n					};\n				})\n			};\n		}\n		validate(offset, bytes) {\n			let low = 0;\n			let high = this.allocations.length;\n			while (low < high) {\n				const mid = low + high >>> 1;\n				const entry = this.allocations[mid];\n				if (entry !== void 0 && entry.offset + entry.bytes.length <= offset) low = mid + 1;\n				else high = mid;\n			}\n			for (let index = low; index < this.allocations.length; index += 1) {\n				const entry = this.allocations[index];\n				if (entry === void 0 || entry.offset >= offset + bytes.length) break;\n				const start = Math.max(offset, entry.offset);\n				const end = Math.min(offset + bytes.length, entry.offset + entry.bytes.length);\n				if (entry.validate?.(start - entry.offset, bytes.subarray(start - offset, end - offset)) === false) return false;\n			}\n			return true;\n		}\n	};\n	function validateAsset(asset) {\n		if (asset.name.length === 0) throw new TypeError("visual asset names cannot be empty");\n		switch (asset.kind) {\n			case "sprite":\n				validateSprite(asset);\n				return;\n			case "animation":\n				if (asset.frames.length === 0) throw new RangeError(`animation \'${asset.name}\' has no frames`);\n				asset.frames.forEach(validateSprite);\n				return;\n			case "tile_set":\n				if (asset.tiles.length === 0 || asset.flags.length !== asset.tiles.length) throw new RangeError(`tile set \'${asset.name}\' has incoherent tiles or flags`);\n				for (const tile of asset.tiles) {\n					validateSprite(tile);\n					if (tile.width !== 8 || tile.height !== 8) throw new RangeError(`tile set \'${asset.name}\' contains a non-8x8 tile`);\n				}\n				return;\n			case "map":\n				if (asset.layers.length === 0) throw new RangeError(`map \'${asset.name}\' has no layers`);\n				for (const layer of asset.layers) if (!Number.isSafeInteger(layer.width) || !Number.isSafeInteger(layer.height) || layer.width <= 0 || layer.height <= 0 || layer.cells.length !== layer.width * layer.height || layer.tileSet.length === 0) throw new RangeError(`map \'${asset.name}\' has an invalid layer`);\n				return;\n			case "font":\n				if (!Number.isSafeInteger(asset.glyphWidth) || !Number.isSafeInteger(asset.glyphHeight) || asset.glyphWidth < 1 || asset.glyphWidth > 16 || asset.glyphHeight < 1 || asset.glyphHeight > 16 || !Number.isSafeInteger(asset.baseline) || asset.baseline < 0 || asset.baseline >= asset.glyphHeight || !Number.isSafeInteger(asset.advanceX) || asset.advanceX < 1 || asset.advanceX > 32 || !Number.isSafeInteger(asset.advanceY) || asset.advanceY < 1 || asset.advanceY > 32 || !Number.isSafeInteger(asset.missingGlyph) || asset.missingGlyph < 0 || asset.missingGlyph > 255 || asset.glyphs.size === 0 || asset.glyphs.size > 256 || !asset.glyphs.has(asset.missingGlyph)) throw new RangeError(`font \'${asset.name}\' has invalid metrics or glyph map`);\n				for (const [code, pixels] of asset.glyphs) if (!Number.isSafeInteger(code) || code < 0 || code > 255 || pixels.length !== asset.glyphWidth * asset.glyphHeight || pixels.some((value) => value > 1)) throw new RangeError(`font \'${asset.name}\' has an invalid glyph`);\n				return;\n		}\n	}\n	function validateSprite(sprite) {\n		if (!Number.isSafeInteger(sprite.width) || !Number.isSafeInteger(sprite.height) || sprite.width < 1 || sprite.width > HARDWARE.spriteMaximumAxis || sprite.height < 1 || sprite.height > HARDWARE.spriteMaximumAxis || sprite.pixels.length !== sprite.width * sprite.height || sprite.pixels.some((color) => color >= HARDWARE.paletteSize)) throw new RangeError(`sprite \'${sprite.name}\' is outside MOD-01 limits`);\n	}\n	function visualAssetBytes(asset) {\n		switch (asset.kind) {\n			case "sprite": return asset.pixels.byteLength;\n			case "animation": return asset.frames.reduce((total, frame) => total + frame.pixels.byteLength, 0);\n			case "tile_set": return asset.flags.byteLength + asset.tiles.reduce((total, tile) => total + tile.pixels.byteLength, 0);\n			case "map": return asset.layers.reduce((total, layer) => total + layer.cells.byteLength, 0);\n			case "font": return 8 + asset.glyphs.size * (2 + asset.glyphWidth * asset.glyphHeight);\n		}\n	}\n	//#endregion\n	//#region src/graphics.ts\n	function isGraphicsSnapshot(value) {\n		const pixels = HARDWARE.width * HARDWARE.height;\n		return isRecord$8(value) && value.revision === 1 && value.front instanceof Uint8Array && value.front.length === pixels && value.front.every((color) => color < HARDWARE.paletteSize) && value.resolved instanceof Uint8Array && value.resolved.length === pixels && value.resolved.every((color) => color < HARDWARE.paletteSize);\n	}\n	/** Deterministic indexed immediate-mode rasterizer with double-buffered storage. */\n	var IndexedGraphics = class {\n		front = new Uint8Array(HARDWARE.width * HARDWARE.height);\n		back = new Uint8Array(HARDWARE.width * HARDWARE.height);\n		resolved = new Uint8Array(HARDWARE.width * HARDWARE.height);\n		assets;\n		display;\n		drawRegisters = /* @__PURE__ */ new Uint8Array(80);\n		transparency = Uint8Array.of(HARDWARE.transparentColor);\n		state = new MemoryDrawState(this.drawRegisters);\n		rasterBytes = new Uint8Array(HARDWARE.height * MEMORY.rasterStride);\n		rasterView = new DataView(this.rasterBytes.buffer);\n		rasterRemaps = Array.from({ length: HARDWARE.height }, (_, line) => this.rasterBytes.subarray(line * MEMORY.rasterStride + 8, (line + 1) * MEMORY.rasterStride));\n		rasterLive = /* @__PURE__ */ new Uint8Array(48);\n		rasterLiveView = new DataView(this.rasterLive.buffer);\n		displayRemap = this.rasterLive.subarray(16);\n		commandCount = 0;\n		activeFrame = false;\n		constructor(assets = new VisualAssetStore(), display) {\n			this.assets = assets;\n			this.display = display === void 0 ? assets.display ?? copyDisplayConfiguration() : copyDisplayConfiguration(display);\n			this.state.clipWidth = HARDWARE.width;\n			this.state.clipHeight = HARDWARE.height;\n			this.state.remap.set(this.display.remap);\n			this.displayRemap.set(identityRemap());\n		}\n		executeFrame(commands) {\n			if (commands.length > HARDWARE.drawCommandsPerFrame) throw new RangeError("draw-command ceiling exceeded");\n			this.beginFrame();\n			for (const command of commands) this.executeCommand(command);\n			return this.finishFrame();\n		}\n		beginFrame() {\n			this.back.set(this.front);\n			this.drawRegisters.fill(0);\n			this.state.clipWidth = HARDWARE.width;\n			this.state.clipHeight = HARDWARE.height;\n			this.state.remap.set(this.display.remap);\n			this.rasterBytes.fill(0);\n			this.rasterLive.fill(0);\n			this.displayRemap.set(identityRemap());\n			this.commandCount = 0;\n			this.activeFrame = true;\n			for (const raster of this.display.raster) {\n				this.rasterRemaps[raster.line]?.set(raster.remap);\n				this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 4, raster.scrollX, true);\n				this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 6, raster.scrollY, true);\n				this.rasterBytes[raster.line * MEMORY.rasterStride] = 1;\n			}\n		}\n		executeCommand(command) {\n			if (!this.activeFrame) throw new Error("graphics frame has not begun");\n			if (this.commandCount >= HARDWARE.drawCommandsPerFrame) throw new RangeError("draw-command ceiling exceeded");\n			this.commandCount += 1;\n			if (command.rasterLine === void 0) {\n				const writesState = [\n					"camera",\n					"clip",\n					"clip_reset",\n					"pal",\n					"pal_reset"\n				].includes(command.name);\n				this.executeDraw(command, writesState ? this.state : this.state.capture());\n				return;\n			}\n			const line = command.rasterLine;\n			if (line < 0 || line >= HARDWARE.height) throw new RangeError("raster command has an invalid scanline");\n			if (command.name === "pal") {\n				const [from, to] = expectIntegers(command, 2);\n				this.displayRemap[expectRemappableColor(from)] = expectColor(to);\n			} else if (command.name === "raster_scroll") {\n				const [x, y] = expectIntegers(command, 2);\n				this.rasterLiveView.setFloat64(0, x, true);\n				this.rasterLiveView.setFloat64(8, y, true);\n			} else throw new TypeError(`\'${command.name}\' is not valid during raster display`);\n			this.rasterView.setInt16(line * MEMORY.rasterStride + 4, clampInt16(this.rasterLiveView.getFloat64(0, true)), true);\n			this.rasterView.setInt16(line * MEMORY.rasterStride + 6, clampInt16(this.rasterLiveView.getFloat64(8, true)), true);\n			this.rasterRemaps[line]?.set(this.displayRemap);\n			this.rasterBytes[line * MEMORY.rasterStride] = 1;\n		}\n		finishFrame() {\n			if (!this.activeFrame) throw new Error("graphics frame has not begun");\n			let previousRemap = identityRemap();\n			let previousScrollX = 0;\n			let previousScrollY = 0;\n			for (let y = 0; y < HARDWARE.height; y += 1) {\n				const lineRemap = this.rasterRemaps[y];\n				if (lineRemap !== void 0 && this.rasterBytes[y * MEMORY.rasterStride] === 1) {\n					previousRemap = lineRemap;\n					previousScrollX = this.rasterView.getInt16(y * MEMORY.rasterStride + 4, true);\n					previousScrollY = this.rasterView.getInt16(y * MEMORY.rasterStride + 6, true);\n				}\n				for (let x = 0; x < HARDWARE.width; x += 1) {\n					const sourceX = wrap(x + previousScrollX, HARDWARE.width);\n					const sourceY = wrap(y + previousScrollY, HARDWARE.height);\n					const color = this.back[sourceY * HARDWARE.width + sourceX] ?? 0;\n					this.resolved[y * HARDWARE.width + x] = previousRemap[color] ?? color;\n				}\n			}\n			this.front.set(this.back);\n			this.activeFrame = false;\n			return {\n				indexedPixels: this.resolved.slice(),\n				commands: this.commandCount\n			};\n		}\n		snapshot() {\n			return {\n				revision: 1,\n				front: this.front.slice(),\n				resolved: this.resolved.slice()\n			};\n		}\n		memoryRegions() {\n			const validate = (_offset, bytes) => bytes.every((color) => color < HARDWARE.paletteSize);\n			return [\n				{\n					name: "front",\n					address: MEMORY.front,\n					bytes: this.front,\n					writable: true,\n					validate\n				},\n				{\n					name: "back",\n					address: MEMORY.back,\n					bytes: this.back,\n					writable: true,\n					validate\n				},\n				{\n					name: "display",\n					address: MEMORY.display,\n					bytes: this.resolved,\n					writable: false,\n					retained: true,\n					validate\n				},\n				{\n					name: "draw state",\n					address: MEMORY.draw,\n					bytes: this.drawRegisters,\n					writable: true,\n					validate: (offset, bytes) => {\n						const candidate = this.drawRegisters.slice();\n						candidate.set(bytes, offset);\n						const view = new DataView(candidate.buffer);\n						for (let field = 0; field < 48; field += 8) if (!Number.isSafeInteger(view.getFloat64(field, true))) return false;\n						return view.getFloat64(32, true) >= 0 && view.getFloat64(40, true) >= 0 && candidate.subarray(48).every((color) => color < HARDWARE.paletteSize);\n					}\n				},\n				{\n					name: "transparency index",\n					address: MEMORY.transparency,\n					bytes: this.transparency,\n					writable: false\n				},\n				{\n					name: "raster callback state",\n					address: MEMORY.rasterLive,\n					bytes: this.rasterLive,\n					writable: false,\n					retained: true,\n					validate: (_offset, bytes) => {\n						const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);\n						return bytes.length === 48 && Number.isSafeInteger(view.getFloat64(0, true)) && Number.isSafeInteger(view.getFloat64(8, true)) && bytes.subarray(16).every((color) => color < HARDWARE.paletteSize);\n					}\n				},\n				{\n					name: "raster table",\n					address: MEMORY.raster,\n					bytes: this.rasterBytes,\n					writable: true,\n					rasterWritable: true,\n					validate: (offset, bytes) => bytes.every((value, index) => {\n						const field = (offset + index) % MEMORY.rasterStride;\n						return field === 0 ? value <= 1 : field < 4 ? value === 0 : field < 8 || value < HARDWARE.paletteSize;\n					})\n				}\n			];\n		}\n		restore(snapshot) {\n			if (!isGraphicsSnapshot(snapshot)) throw new TypeError("invalid indexed graphics snapshot");\n			this.front.set(snapshot.front);\n			this.back.set(snapshot.front);\n			this.resolved.set(snapshot.resolved);\n			this.activeFrame = false;\n		}\n		executeDraw(command, state) {\n			switch (command.name) {\n				case "clear": {\n					const [color] = expectIntegers(command, 1);\n					this.back.fill(state.remap[expectColor(color)] ?? color);\n					return;\n				}\n				case "pixel": {\n					const [x, y, color] = expectIntegers(command, 3);\n					this.plot(x, y, color, state);\n					return;\n				}\n				case "line": {\n					const [x0, y0, x1, y1, color] = expectIntegers(command, 5);\n					this.line(x0, y0, x1, y1, color, state);\n					return;\n				}\n				case "rect":\n				case "rect_fill": {\n					const [x, y, width, height, color] = expectIntegers(command, 5);\n					this.rectangle(x, y, width, height, color, command.name === "rect_fill", state);\n					return;\n				}\n				case "circle":\n				case "circle_fill": {\n					const [x, y, radius, color] = expectIntegers(command, 4);\n					this.circle(x, y, radius, color, command.name === "circle_fill", state);\n					return;\n				}\n				case "triangle": {\n					const [x0, y0, x1, y1, x2, y2, color] = expectIntegers(command, 7);\n					this.triangle(x0, y0, x1, y1, x2, y2, color, state);\n					return;\n				}\n				case "camera":\n					[state.cameraX, state.cameraY] = expectIntegers(command, 2);\n					return;\n				case "clip": {\n					const [x, y, width, height] = expectIntegers(command, 4);\n					state.clipX = x;\n					state.clipY = y;\n					state.clipWidth = Math.max(0, width);\n					state.clipHeight = Math.max(0, height);\n					return;\n				}\n				case "clip_reset":\n					expectIntegers(command, 0);\n					state.clipX = 0;\n					state.clipY = 0;\n					state.clipWidth = HARDWARE.width;\n					state.clipHeight = HARDWARE.height;\n					return;\n				case "pal": {\n					const [from, to] = expectIntegers(command, 2);\n					state.remap[expectRemappableColor(from)] = expectColor(to);\n					return;\n				}\n				case "pal_reset":\n					expectIntegers(command, 0);\n					state.remap.set(identityRemap());\n					return;\n				case "sprite": {\n					const [handle, x, y] = command.arguments;\n					this.drawSprite(readAssetName$1(handle, "Sprite"), expectInteger$1(x), expectInteger$1(y), state);\n					return;\n				}\n				case "animation": {\n					const [handle, frame, x, y] = command.arguments;\n					this.drawAnimation(readAssetName$1(handle, "Animation"), expectInteger$1(frame), expectInteger$1(x), expectInteger$1(y), state);\n					return;\n				}\n				case "sprite_xform": {\n					const [handle, x, y, scale, quarterTurns, flipX, flipY] = command.arguments;\n					this.drawTransformedSprite(readAssetName$1(handle, "Sprite"), expectInteger$1(x), expectInteger$1(y), expectInteger$1(scale), expectInteger$1(quarterTurns), expectBoolean(flipX), expectBoolean(flipY), state);\n					return;\n				}\n				case "map": {\n					const [handle, x, y] = command.arguments;\n					this.drawMap(readAssetName$1(handle, "Map"), expectInteger$1(x), expectInteger$1(y), state);\n					return;\n				}\n				case "raster_scroll": throw new TypeError("raster_scroll is only valid in the raster callback");\n				case "print": {\n					const [text, x, y, color] = command.arguments;\n					this.print(expectText(text), expectInteger$1(x), expectInteger$1(y), expectInteger$1(color), state);\n					return;\n				}\n				case "font_print": {\n					const [handle, text, x, y, color] = command.arguments;\n					this.printFont(readAssetName$1(handle, "Font"), expectText(text), expectInteger$1(x), expectInteger$1(y), expectInteger$1(color), state);\n					return;\n				}\n				default: throw new TypeError(`unknown graphics command \'${command.name}\'`);\n			}\n		}\n		plot(x, y, color, state) {\n			const screenX = x - state.cameraX;\n			const screenY = y - state.cameraY;\n			if (screenX < 0 || screenX >= HARDWARE.width || screenY < 0 || screenY >= HARDWARE.height || screenX < state.clipX || screenX >= state.clipX + state.clipWidth || screenY < state.clipY || screenY >= state.clipY + state.clipHeight) return;\n			this.back[screenY * HARDWARE.width + screenX] = state.remap[expectColor(color)] ?? color;\n		}\n		line(startX, startY, endX, endY, color, state) {\n			let x = startX;\n			let y = startY;\n			const deltaX = Math.abs(endX - startX);\n			const stepX = startX < endX ? 1 : -1;\n			const deltaY = -Math.abs(endY - startY);\n			const stepY = startY < endY ? 1 : -1;\n			let error = deltaX + deltaY;\n			for (;;) {\n				this.plot(x, y, color, state);\n				if (x === endX && y === endY) return;\n				const doubled = error * 2;\n				if (doubled >= deltaY) {\n					error += deltaY;\n					x += stepX;\n				}\n				if (doubled <= deltaX) {\n					error += deltaX;\n					y += stepY;\n				}\n			}\n		}\n		rectangle(x, y, width, height, color, filled, state) {\n			if (width <= 0 || height <= 0) return;\n			if (!filled) {\n				this.line(x, y, x + width - 1, y, color, state);\n				this.line(x, y + height - 1, x + width - 1, y + height - 1, color, state);\n				this.line(x, y, x, y + height - 1, color, state);\n				this.line(x + width - 1, y, x + width - 1, y + height - 1, color, state);\n				return;\n			}\n			for (let row = 0; row < height; row += 1) this.line(x, y + row, x + width - 1, y + row, color, state);\n		}\n		circle(centerX, centerY, radius, color, filled, state) {\n			if (radius < 0) return;\n			let x = radius;\n			let y = 0;\n			let error = 1 - radius;\n			while (x >= y) {\n				if (filled) {\n					this.line(centerX - x, centerY + y, centerX + x, centerY + y, color, state);\n					this.line(centerX - x, centerY - y, centerX + x, centerY - y, color, state);\n					this.line(centerX - y, centerY + x, centerX + y, centerY + x, color, state);\n					this.line(centerX - y, centerY - x, centerX + y, centerY - x, color, state);\n				} else for (const [plotX, plotY] of [\n					[centerX + x, centerY + y],\n					[centerX + y, centerY + x],\n					[centerX - y, centerY + x],\n					[centerX - x, centerY + y],\n					[centerX - x, centerY - y],\n					[centerX - y, centerY - x],\n					[centerX + y, centerY - x],\n					[centerX + x, centerY - y]\n				]) this.plot(plotX, plotY, color, state);\n				y += 1;\n				if (error < 0) error += 2 * y + 1;\n				else {\n					x -= 1;\n					error += 2 * (y - x) + 1;\n				}\n			}\n		}\n		triangle(x0, y0, x1, y1, x2, y2, color, state) {\n			const minimumX = Math.min(x0, x1, x2);\n			const maximumX = Math.max(x0, x1, x2);\n			const minimumY = Math.min(y0, y1, y2);\n			const maximumY = Math.max(y0, y1, y2);\n			const area = edge(x0, y0, x1, y1, x2, y2);\n			if (area === 0) {\n				this.line(x0, y0, x1, y1, color, state);\n				this.line(x1, y1, x2, y2, color, state);\n				return;\n			}\n			for (let y = minimumY; y <= maximumY; y += 1) for (let x = minimumX; x <= maximumX; x += 1) {\n				const first = edge(x1, y1, x2, y2, x, y);\n				const second = edge(x2, y2, x0, y0, x, y);\n				const third = edge(x0, y0, x1, y1, x, y);\n				if (area > 0 && first >= 0 && second >= 0 && third >= 0 || area < 0 && first <= 0 && second <= 0 && third <= 0) this.plot(x, y, color, state);\n			}\n		}\n		drawSprite(name, x, y, state) {\n			const asset = this.assets.get(name);\n			if (asset?.kind !== "sprite") throw new TypeError(`missing Sprite asset \'${name}\'`);\n			this.blit(asset, x, y, 1, 0, false, false, state);\n		}\n		drawAnimation(name, frame, x, y, state) {\n			const asset = this.assets.get(name);\n			if (asset?.kind !== "animation" || asset.frames.length === 0) throw new TypeError(`missing Animation asset \'${name}\'`);\n			const selected = asset.frames[wrap(frame, asset.frames.length)];\n			if (selected !== void 0) this.blit(selected, x, y, 1, 0, false, false, state);\n		}\n		drawTransformedSprite(name, x, y, scale, quarterTurns, flipX, flipY, state) {\n			const asset = this.assets.get(name);\n			if (asset?.kind !== "sprite") throw new TypeError(`missing Sprite asset \'${name}\'`);\n			if (scale < 1 || scale > 16) throw new RangeError("sprite scale must be between 1 and 16");\n			this.blit(asset, x, y, scale, wrap(quarterTurns, 4), flipX, flipY, state);\n		}\n		blit(sprite, x, y, scale, quarterTurns, flipX, flipY, state) {\n			const outputWidth = (quarterTurns % 2 === 0 ? sprite.width : sprite.height) * scale;\n			const outputHeight = (quarterTurns % 2 === 0 ? sprite.height : sprite.width) * scale;\n			for (let outputY = 0; outputY < outputHeight; outputY += 1) for (let outputX = 0; outputX < outputWidth; outputX += 1) {\n				let sourceX = Math.floor(outputX / scale);\n				let sourceY = Math.floor(outputY / scale);\n				[sourceX, sourceY] = unrotate(sourceX, sourceY, sprite.width, sprite.height, quarterTurns);\n				if (flipX) sourceX = sprite.width - 1 - sourceX;\n				if (flipY) sourceY = sprite.height - 1 - sourceY;\n				const color = sprite.pixels[sourceY * sprite.width + sourceX] ?? HARDWARE.transparentColor;\n				if (color !== this.transparency[0]) this.plot(x + outputX, y + outputY, color, state);\n			}\n		}\n		drawMap(name, x, y, state) {\n			const map = this.assets.get(name);\n			if (map?.kind !== "map") throw new TypeError(`missing Map asset \'${name}\'`);\n			for (const layer of map.layers) {\n				const tileSet = this.assets.get(layer.tileSet);\n				if (tileSet?.kind !== "tile_set") throw new TypeError(`missing TileSet asset \'${layer.tileSet}\'`);\n				const clipRight = Math.min(HARDWARE.width, state.clipX + state.clipWidth);\n				const clipBottom = Math.min(HARDWARE.height, state.clipY + state.clipHeight);\n				const firstColumn = Math.max(0, Math.floor((state.clipX + state.cameraX - x) / HARDWARE.tileSize));\n				const lastColumn = Math.min(layer.width, Math.ceil((clipRight + state.cameraX - x) / HARDWARE.tileSize));\n				const firstRow = Math.max(0, Math.floor((state.clipY + state.cameraY - y) / HARDWARE.tileSize));\n				const lastRow = Math.min(layer.height, Math.ceil((clipBottom + state.cameraY - y) / HARDWARE.tileSize));\n				for (let row = firstRow; row < lastRow; row += 1) for (let column = firstColumn; column < lastColumn; column += 1) {\n					const tile = tileSet.tiles[layer.cells.getUint16((row * layer.width + column) * 2, true)];\n					if (tile !== void 0) this.blit(tile, x + column * HARDWARE.tileSize, y + row * HARDWARE.tileSize, 1, 0, false, false, state);\n				}\n			}\n		}\n		print(text, x, y, color, state) {\n			let cursorX = x;\n			let cursorY = y;\n			for (const character of text) {\n				if (character === "\\n") {\n					cursorX = x;\n					cursorY += BITMAP_FONT.advanceY;\n					continue;\n				}\n				glyphRows(character).forEach((bits, row) => {\n					for (let column = 0; column < BITMAP_FONT.glyphWidth; column += 1) if ((bits & 1 << BITMAP_FONT.glyphWidth - 1 - column) !== 0) this.plot(cursorX + column, cursorY + row, color, state);\n				});\n				cursorX += BITMAP_FONT.advanceX;\n			}\n		}\n		printFont(name, text, x, y, color, state) {\n			const font = this.assets.get(name);\n			if (font?.kind !== "font") throw new TypeError(`missing Font asset \'${name}\'`);\n			let cursorX = x;\n			let cursorY = y;\n			for (const character of text) {\n				if (character === "\\n") {\n					cursorX = x;\n					cursorY += font.advanceY;\n					continue;\n				}\n				const code = character.codePointAt(0) ?? font.missingGlyph;\n				const glyph = font.glyphs.get(code) ?? font.glyphs.get(font.missingGlyph);\n				if (glyph !== void 0) {\n					for (let row = 0; row < font.glyphHeight; row += 1) for (let column = 0; column < font.glyphWidth; column += 1) if (glyph[row * font.glyphWidth + column] === 1) this.plot(cursorX + column, cursorY + row, color, state);\n				}\n				cursorX += font.advanceX;\n			}\n		}\n	};\n	/** Deterministic four-by-four Bayer choice between two palette indices. */\n	function orderedDither(x, y, first, second, level) {\n		return ([\n			0,\n			8,\n			2,\n			10,\n			12,\n			4,\n			14,\n			6,\n			3,\n			11,\n			1,\n			9,\n			15,\n			7,\n			13,\n			5\n		][wrap(y, 4) * 4 + wrap(x, 4)] ?? 0) < Math.max(0, Math.min(16, level)) ? expectColor(second) : expectColor(first);\n	}\n	var MemoryDrawState = class {\n		view;\n		remap;\n		constructor(bytes) {\n			this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);\n			this.remap = bytes.subarray(48);\n		}\n		capture() {\n			return {\n				cameraX: this.cameraX,\n				cameraY: this.cameraY,\n				clipX: this.clipX,\n				clipY: this.clipY,\n				clipWidth: this.clipWidth,\n				clipHeight: this.clipHeight,\n				remap: this.remap\n			};\n		}\n		get cameraX() {\n			return this.view.getFloat64(0, true);\n		}\n		set cameraX(value) {\n			this.view.setFloat64(0, value, true);\n		}\n		get cameraY() {\n			return this.view.getFloat64(8, true);\n		}\n		set cameraY(value) {\n			this.view.setFloat64(8, value, true);\n		}\n		get clipX() {\n			return this.view.getFloat64(16, true);\n		}\n		set clipX(value) {\n			this.view.setFloat64(16, value, true);\n		}\n		get clipY() {\n			return this.view.getFloat64(24, true);\n		}\n		set clipY(value) {\n			this.view.setFloat64(24, value, true);\n		}\n		get clipWidth() {\n			return this.view.getFloat64(32, true);\n		}\n		set clipWidth(value) {\n			this.view.setFloat64(32, value, true);\n		}\n		get clipHeight() {\n			return this.view.getFloat64(40, true);\n		}\n		set clipHeight(value) {\n			this.view.setFloat64(40, value, true);\n		}\n	};\n	function copyDisplayConfiguration(display) {\n		if (display === void 0) return {\n			remap: identityRemap(),\n			raster: []\n		};\n		if (!validRemap(display.remap)) throw new TypeError("display configuration has an invalid base remap");\n		let previousLine = -1;\n		const raster = display.raster.map((state) => {\n			if (!Number.isInteger(state.line) || state.line <= previousLine || state.line >= HARDWARE.height || !Number.isSafeInteger(state.scrollX) || !Number.isSafeInteger(state.scrollY) || state.scrollX < -32768 || state.scrollX > 32767 || state.scrollY < -32768 || state.scrollY > 32767 || !validRemap(state.remap)) throw new TypeError("display configuration has invalid raster state");\n			previousLine = state.line;\n			return {\n				...state,\n				remap: state.remap.slice()\n			};\n		});\n		return {\n			remap: display.remap.slice(),\n			raster\n		};\n	}\n	function validRemap(remap) {\n		return remap.length === HARDWARE.rasterPaletteSize && remap.every((color) => color < HARDWARE.paletteSize);\n	}\n	function isRecord$8(value) {\n		return typeof value === "object" && value !== null;\n	}\n	function identityRemap() {\n		return Uint8Array.from({ length: HARDWARE.rasterPaletteSize }, (_, index) => index);\n	}\n	function expectIntegers(command, count) {\n		if (command.arguments.length !== count) throw new TypeError(`${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`);\n		return command.arguments.map(expectInteger$1);\n	}\n	function expectInteger$1(value) {\n		if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new TypeError("graphics arguments must be safe integers");\n		return value;\n	}\n	function expectBoolean(value) {\n		if (typeof value !== "boolean") throw new TypeError("graphics argument must be Bool");\n		return value;\n	}\n	function expectText(value) {\n		if (typeof value !== "string") throw new TypeError("graphics argument must be Text");\n		return value;\n	}\n	function expectColor(value) {\n		if (value < 0 || value >= HARDWARE.paletteSize) throw new RangeError("palette index must be between 0 and 255");\n		return value;\n	}\n	function expectRemappableColor(value) {\n		if (value < 0 || value >= HARDWARE.rasterPaletteSize) throw new RangeError("palette remap source must be between 0 and 31");\n		return value;\n	}\n	function readAssetName$1(value, expectedKind) {\n		if (typeof value !== "object" || value === null || !("name" in value) || typeof value.name !== "string" || !("kind" in value) || value.kind !== expectedKind) throw new TypeError(`expected a ${expectedKind} asset handle`);\n		return value.name;\n	}\n	function edge(firstX, firstY, secondX, secondY, pointX, pointY) {\n		return (pointX - firstX) * (secondY - firstY) - (pointY - firstY) * (secondX - firstX);\n	}\n	function unrotate(x, y, width, height, quarterTurns) {\n		switch (quarterTurns) {\n			case 1: return [y, height - 1 - x];\n			case 2: return [width - 1 - x, height - 1 - y];\n			case 3: return [width - 1 - y, x];\n			default: return [x, y];\n		}\n	}\n	function wrap(value, modulus) {\n		return (value % modulus + modulus) % modulus;\n	}\n	function clampInt16(value) {\n		return Math.max(-32768, Math.min(32767, value));\n	}\n	//#endregion\n	//#region src/input.ts\n	const BUTTONS = [\n		"up",\n		"down",\n		"left",\n		"right",\n		"a",\n		"b",\n		"x",\n		"y",\n		"l",\n		"r",\n		"start",\n		"menu"\n	];\n	function emptyInputFrame() {\n		const controller = () => ({ buttons: Object.fromEntries(BUTTONS.map((button) => [button, false])) });\n		return {\n			controllers: [\n				controller(),\n				controller(),\n				controller(),\n				controller()\n			],\n			pointer: {\n				x: 0,\n				y: 0,\n				primary: false,\n				secondary: false,\n				inside: false\n			}\n		};\n	}\n	/** Little-endian controller/pointer MMIO over the same frames used by the high-level API. */\n	function inputRegisterByte(current, previous, offset) {\n		if (offset < 0 || offset >= 48 || !Number.isInteger(offset)) return 0;\n		if (offset < 32) {\n			const port = Math.floor(offset / 8);\n			const mask = (frame) => BUTTONS.reduce((bits, button, bit) => bits | (frame.controllers[port]?.buttons[button] ? 1 << bit : 0), 0);\n			const held = mask(current);\n			const before = mask(previous);\n			const field = offset % 8;\n			return (field < 2 ? held : field < 4 ? before : field < 6 ? held & ~before : before & ~held) >>> offset % 2 * 8 & 255;\n		}\n		const field = offset - 32;\n		if (field < 8) {\n			const pointer = field < 4 ? current.pointer : previous.pointer;\n			return (field % 4 < 2 ? pointer.x : pointer.y) >>> field % 2 * 8 & 255;\n		}\n		const flags = (pointer) => Number(pointer.primary) | Number(pointer.secondary) << 1 | Number(pointer.inside) << 2;\n		const held = flags(current.pointer);\n		const before = flags(previous.pointer);\n		return field === 8 ? held : field === 9 ? before : field === 10 ? held & ~before : field === 11 ? before & ~held : 0;\n	}\n	function isButton(value) {\n		return typeof value === "string" && BUTTONS.includes(value);\n	}\n	function isInputFrame(value) {\n		if (!isRecord$7(value) || !hasExactKeys$1(value, ["controllers", "pointer"]) || !Array.isArray(value.controllers) || value.controllers.length !== 4) return false;\n		if (Object.keys(value.controllers).length !== 4 || !Array.from(value.controllers).every(isControllerState) || !isRecord$7(value.pointer)) return false;\n		const pointer = value.pointer;\n		return hasExactKeys$1(pointer, [\n			"x",\n			"y",\n			"primary",\n			"secondary",\n			"inside"\n		]) && typeof pointer.x === "number" && Number.isSafeInteger(pointer.x) && pointer.x >= 0 && pointer.x < HARDWARE.width && typeof pointer.y === "number" && Number.isSafeInteger(pointer.y) && pointer.y >= 0 && pointer.y < HARDWARE.height && typeof pointer.primary === "boolean" && typeof pointer.secondary === "boolean" && typeof pointer.inside === "boolean";\n	}\n	Object.freeze({\n		ArrowUp: [0, "up"],\n		ArrowDown: [0, "down"],\n		ArrowLeft: [0, "left"],\n		ArrowRight: [0, "right"],\n		KeyZ: [0, "a"],\n		KeyX: [0, "b"],\n		KeyA: [0, "x"],\n		KeyS: [0, "y"],\n		KeyQ: [0, "l"],\n		KeyW: [0, "r"],\n		Enter: [0, "start"],\n		Escape: [0, "menu"],\n		KeyI: [1, "up"],\n		KeyK: [1, "down"],\n		KeyJ: [1, "left"],\n		KeyL: [1, "right"],\n		KeyF: [1, "a"],\n		KeyG: [1, "b"],\n		KeyR: [1, "x"],\n		KeyT: [1, "y"],\n		KeyV: [1, "l"],\n		KeyB: [1, "r"],\n		Digit1: [1, "start"],\n		Backquote: [1, "menu"],\n		Numpad8: [2, "up"],\n		Numpad5: [2, "down"],\n		Numpad4: [2, "left"],\n		Numpad6: [2, "right"],\n		Numpad1: [2, "a"],\n		Numpad2: [2, "b"],\n		Numpad7: [2, "x"],\n		Numpad9: [2, "y"],\n		NumpadAdd: [2, "l"],\n		NumpadSubtract: [2, "r"],\n		NumpadEnter: [2, "start"],\n		NumpadDecimal: [2, "menu"],\n		KeyY: [3, "up"],\n		KeyH: [3, "down"],\n		KeyU: [3, "left"],\n		KeyO: [3, "right"],\n		KeyC: [3, "a"],\n		KeyD: [3, "b"],\n		KeyE: [3, "x"],\n		KeyM: [3, "y"],\n		KeyN: [3, "l"],\n		KeyP: [3, "r"],\n		Digit2: [3, "start"],\n		Digit3: [3, "menu"]\n	});\n	Object.freeze({\n		up: 12,\n		down: 13,\n		left: 14,\n		right: 15,\n		a: 0,\n		b: 1,\n		x: 2,\n		y: 3,\n		l: 4,\n		r: 5,\n		start: 9,\n		menu: 8\n	});\n	function isControllerState(value) {\n		if (!isRecord$7(value) || !hasExactKeys$1(value, ["buttons"]) || !isRecord$7(value.buttons)) return false;\n		const buttons = value.buttons;\n		return hasExactKeys$1(buttons, BUTTONS) && BUTTONS.every((button) => typeof buttons[button] === "boolean");\n	}\n	function isRecord$7(value) {\n		return typeof value === "object" && value !== null;\n	}\n	function hasExactKeys$1(value, expected) {\n		const keys = Object.keys(value);\n		return keys.length === expected.length && expected.every((key) => keys.includes(key));\n	}\n	//#endregion\n	//#region src/rng.ts\n	const NON_ZERO_FALLBACK = 1295003697;\n	/** Console-owned xorshift32 stream with an explicit serializable state. */\n	var DeterministicRng = class {\n		current;\n		constructor(seed = NON_ZERO_FALLBACK) {\n			this.current = normalizeSeed(seed);\n		}\n		get state() {\n			return this.current >>> 0;\n		}\n		restore(state) {\n			this.current = normalizeSeed(state);\n		}\n		nextU32() {\n			let value = this.current >>> 0;\n			value ^= value << 13;\n			value ^= value >>> 17;\n			value ^= value << 5;\n			this.current = value >>> 0;\n			return this.current;\n		}\n		nextNum() {\n			return this.nextU32() / 4294967296;\n		}\n		nextInt(minimum, maximumExclusive) {\n			if (!Number.isSafeInteger(minimum) || !Number.isSafeInteger(maximumExclusive) || maximumExclusive <= minimum) throw new RangeError("rng_int bounds must be safe integers with maximum greater than minimum");\n			const range = maximumExclusive - minimum;\n			if (range > 4294967296) throw new RangeError("rng_int range must not exceed 2^32");\n			const rejectionLimit = 4294967296 - 4294967296 % range;\n			let sample;\n			do\n				sample = this.nextU32();\n			while (sample >= rejectionLimit);\n			return minimum + sample % range;\n		}\n	};\n	function normalizeSeed(seed) {\n		if (!Number.isSafeInteger(seed)) throw new RangeError("RNG seed must be a safe integer");\n		const normalized = seed >>> 0;\n		return normalized === 0 ? NON_ZERO_FALLBACK : normalized;\n	}\n	//#endregion\n	//#region src/system.ts\n	const EXECUTION_PHASES = [\n		"idle",\n		"start",\n		"update",\n		"draw",\n		"raster",\n		"output"\n	];\n	function isExecutionSnapshot(value, frame, rate, budget) {\n		if (!isRecord$6(value) || Object.keys(value).length !== 5 || typeof value.booted !== "boolean" || typeof value.updates !== "number" || !Number.isSafeInteger(value.updates) || value.updates < 0 || typeof value.phase !== "string" || !EXECUTION_PHASES.includes(value.phase) || (value.phase === "raster" ? typeof value.rasterLine !== "number" || !Number.isInteger(value.rasterLine) || value.rasterLine < 0 || value.rasterLine >= HARDWARE.height : value.rasterLine !== null) || value.fault !== null && !isMachineFault(value.fault)) return false;\n		const base = rate === 60 ? frame : Math.ceil(frame / 2);\n		const updated = (value.phase === "draw" || value.phase === "raster" || value.phase === "output") && (rate === 60 || frame % 2 === 0);\n		if (value.updates !== base + Number(updated) || value.phase !== "idle" && frame === Number.MAX_SAFE_INTEGER || !value.booted && (frame !== 0 || value.phase !== "idle" && value.phase !== "start") || value.phase === "start" && value.booted || value.phase === "update" && rate === 30 && frame % 2 !== 0) return false;\n		return value.fault !== null || value.phase === "idle" && budget.used <= budget.limit;\n	}\n	function isMachineFault(value) {\n		return isRecord$6(value) && Object.keys(value).length === 2 && typeof value.code === "number" && Number.isInteger(value.code) && value.code >= 9e3 && value.code <= 9999 && isFaultSpan(value.sourceSpan);\n	}\n	function isFaultSpan(value) {\n		return isRecord$6(value) && Object.keys(value).length === 2 && typeof value.start === "number" && Number.isInteger(value.start) && value.start >= 0 && typeof value.end === "number" && Number.isInteger(value.end) && value.end >= value.start && value.end <= 4294967295;\n	}\n	function isRecord$6(value) {\n		return typeof value === "object" && value !== null && !Array.isArray(value);\n	}\n	/** Wire encoding of current device-owned state, not a retained register image. */\n	function systemRegisterByte(state, offset) {\n		if (!Number.isInteger(offset) || offset < 0 || offset >= 64) return 0;\n		const byte = (value, index) => Math.floor(value / 2 ** (index * 8)) & 255;\n		if (offset < 8) return byte(state.frame, offset);\n		if (offset < 16) return byte(state.updates, offset - 8);\n		if (offset < 24) {\n			const encoded = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));\n			encoded.setFloat64(0, state.seconds, true);\n			return encoded.getUint8(offset - 16);\n		}\n		if (offset < 28) return byte(state.rngState, offset - 24);\n		if (offset === 28) return state.updateRate;\n		if (offset === 29) return EXECUTION_PHASES.indexOf(state.phase);\n		if (offset < 32) return byte(state.rasterLine ?? 65535, offset - 30);\n		if (offset < 40) return byte(state.used, offset - 32);\n		if (offset < 48) return byte(state.limit, offset - 40);\n		if (offset === 48) return Number(state.booted) | Number(state.phase !== "idle" && state.fault === null) << 1 | Number(state.fault !== null) << 2;\n		if (offset < 52) return 0;\n		if (offset < 54) return byte(state.fault?.code ?? 0, offset - 52);\n		if (offset < 56) return 0;\n		if (offset < 60) return byte(state.fault?.sourceSpan.start ?? 0, offset - 56);\n		return byte(state.fault?.sourceSpan.end ?? 0, offset - 60);\n	}\n	//#endregion\n	//#region src/machine.ts\n	/** Deterministic callback scheduler and the only API surface visible to generated cartridge code. */\n	var DeterministicMachine = class {\n		budget;\n		rng;\n		updateRate;\n		cartridge;\n		hooks;\n		debugEnabled;\n		currentFrame = 0;\n		booted = false;\n		input = emptyInputFrame();\n		previousInput = emptyInputFrame();\n		phase = "idle";\n		rasterLine;\n		completedUpdates = 0;\n		lastFault = null;\n		debugInput;\n		constructor(factory, configuration, hooks = {}) {\n			if (![30, 60].includes(configuration.updateRate)) throw new RangeError("update rate must be 30 or 60 Hz");\n			this.budget = new WorkBudget(configuration.workUnitsPerFrame);\n			this.rng = new DeterministicRng(configuration.seed);\n			this.updateRate = configuration.updateRate;\n			this.debugEnabled = configuration.debug === true;\n			this.hooks = hooks;\n			this.cartridge = factory(this);\n		}\n		get frame() {\n			return this.currentFrame;\n		}\n		get cartridgeTimeSeconds() {\n			return this.currentFrame / 60;\n		}\n		boot() {\n			this.assertRunnable();\n			if (this.booted) return;\n			this.budget.beginFrame();\n			this.phase = "start";\n			this.rasterLine = void 0;\n			try {\n				this.completeDebugCallback(() => this.cartridge.start());\n				this.booted = true;\n				this.phase = "idle";\n			} catch (error) {\n				this.rememberFault(error);\n				throw error;\n			}\n		}\n		runFrame(input) {\n			if (!isInputFrame(input)) throw new RuntimeFault("M019008", "invalid controller input frame", {\n				start: 0,\n				end: 0\n			});\n			this.assertRunnable();\n			if (this.debugEnabled) for (;;) {\n				const step = this.stepDebug(input);\n				if (step.report !== void 0) return step.report;\n			}\n			if (!this.booted) this.boot();\n			this.previousInput = this.input;\n			this.input = structuredClone(input);\n			this.budget.beginFrame();\n			try {\n				if (this.updateRate === 60 || this.currentFrame % 2 === 0) {\n					this.phase = "update";\n					this.completeDebugCallback(() => this.cartridge.update());\n					this.completedUpdates += 1;\n				}\n				this.phase = "draw";\n				this.completeDebugCallback(() => this.cartridge.draw());\n				for (let line = 0; line < 144; line += 1) {\n					this.phase = "raster";\n					this.rasterLine = line;\n					this.completeDebugCallback(() => this.cartridge.raster(line));\n				}\n				this.rasterLine = void 0;\n				this.phase = "output";\n				this.hooks.completeFrame?.();\n				const report = {\n					frame: this.currentFrame,\n					workUnits: this.budget.used,\n					attribution: this.budget.attribution()\n				};\n				this.currentFrame += 1;\n				this.phase = "idle";\n				return report;\n			} catch (error) {\n				this.rememberFault(error);\n				throw error;\n			}\n		}\n		/** Advances a debug build to exactly one statement boundary or one completed frame. */\n		stepDebug(input) {\n			if (!this.debugEnabled) throw new RuntimeFault("M019015", "statement stepping requires a debug cartridge", {\n				start: 0,\n				end: 0\n			});\n			if (!isInputFrame(input)) throw new RuntimeFault("M019008", "invalid controller input frame", {\n				start: 0,\n				end: 0\n			});\n			if (this.lastFault !== null) this.assertRunnable();\n			try {\n				if (this.phase === "idle") {\n					this.debugInput = structuredClone(input);\n					this.budget.beginFrame();\n					if (!this.booted) this.phase = "start";\n					else this.beginDebugFrame();\n				}\n				for (;;) {\n					const step = this.stepDebugPhase();\n					if (step.event !== void 0) {\n						this.probe(step.event.id, step.event.sourceSpan, step.event.locals);\n						return { event: structuredClone(step.event) };\n					}\n					if (!step.done) throw new TypeError("invalid generated debug step");\n					const boundary = this.advanceDebugPhase();\n					if (boundary === "booted") return { booted: true };\n					if (boundary !== void 0) return { report: boundary };\n				}\n			} catch (error) {\n				this.rememberFault(error);\n				throw error;\n			}\n		}\n		beginDebugFrame() {\n			const input = this.debugInput ?? emptyInputFrame();\n			this.previousInput = this.input;\n			this.input = structuredClone(input);\n			this.phase = this.updateRate === 60 || this.currentFrame % 2 === 0 ? "update" : "draw";\n		}\n		stepDebugPhase() {\n			switch (this.phase) {\n				case "start": return this.cartridge.start() ?? { done: true };\n				case "update": return this.cartridge.update() ?? { done: true };\n				case "draw": return this.cartridge.draw() ?? { done: true };\n				case "raster": return this.cartridge.raster(this.rasterLine ?? 0) ?? { done: true };\n				case "idle":\n				case "output": throw new TypeError(`cannot step machine phase ${this.phase}`);\n			}\n		}\n		advanceDebugPhase() {\n			switch (this.phase) {\n				case "start":\n					this.booted = true;\n					this.phase = "idle";\n					this.debugInput = void 0;\n					return "booted";\n				case "update":\n					this.completedUpdates += 1;\n					this.phase = "draw";\n					return;\n				case "draw":\n					this.phase = "raster";\n					this.rasterLine = 0;\n					return;\n				case "raster": {\n					if ((this.rasterLine ?? 0) < 143) {\n						this.rasterLine = (this.rasterLine ?? 0) + 1;\n						return;\n					}\n					this.rasterLine = void 0;\n					this.phase = "output";\n					this.hooks.completeFrame?.();\n					const report = {\n						frame: this.currentFrame,\n						workUnits: this.budget.used,\n						attribution: this.budget.attribution()\n					};\n					this.currentFrame += 1;\n					this.phase = "idle";\n					this.debugInput = void 0;\n					return report;\n				}\n				case "idle":\n				case "output": throw new TypeError(`cannot advance machine phase ${this.phase}`);\n			}\n		}\n		completeDebugCallback(callback) {\n			for (;;) {\n				const step = callback();\n				if (step === void 0 || step.done) return;\n				if (step.event !== void 0) this.probe(step.event.id, step.event.sourceSpan, step.event.locals);\n			}\n		}\n		snapshot() {\n			if (this.phase !== "idle" && this.lastFault === null) throw new TypeError("machine snapshots require a completed frame or fault boundary");\n			return structuredClone({\n				revision: 2,\n				frame: this.currentFrame,\n				rngState: this.rng.state,\n				cartridge: this.cartridge.snapshot(),\n				input: this.input,\n				previousInput: this.previousInput,\n				updateRate: this.updateRate,\n				budget: this.budget.snapshot(),\n				execution: {\n					booted: this.booted,\n					updates: this.completedUpdates,\n					phase: this.phase,\n					rasterLine: this.rasterLine ?? null,\n					fault: this.lastFault\n				}\n			});\n		}\n		restore(value) {\n			if (!isMachineSnapshot(value)) throw new TypeError("invalid MOD-01 machine snapshot");\n			const snapshot = value;\n			if (snapshot.revision === 2 && (snapshot.updateRate !== this.updateRate || snapshot.budget.limit !== this.budget.limit)) throw new TypeError("machine snapshot does not match the execution configuration");\n			const previous = structuredClone(this.cartridge.snapshot());\n			try {\n				this.cartridge.restore(structuredClone(snapshot.cartridge));\n			} catch (error) {\n				this.cartridge.restore(previous);\n				throw error;\n			}\n			if (snapshot.revision === 2) {\n				this.budget.restore(snapshot.budget);\n				this.booted = snapshot.execution.booted;\n				this.completedUpdates = snapshot.execution.updates;\n				this.phase = snapshot.execution.phase;\n				this.rasterLine = snapshot.execution.rasterLine ?? void 0;\n				this.lastFault = structuredClone(snapshot.execution.fault);\n			} else {\n				this.budget.beginFrame();\n				this.booted = this.booted || snapshot.frame > 0;\n				this.completedUpdates = this.updateRate === 60 ? snapshot.frame : Math.ceil(snapshot.frame / 2);\n				this.phase = "idle";\n				this.rasterLine = void 0;\n				this.lastFault = null;\n			}\n			this.currentFrame = snapshot.frame;\n			this.rng.restore(snapshot.rngState);\n			this.input = structuredClone(snapshot.input);\n			this.previousInput = structuredClone(snapshot.previousInput);\n			this.debugInput = void 0;\n		}\n		inspect() {\n			return this.cartridge.inspect();\n		}\n		readInputByte(offset) {\n			return inputRegisterByte(this.input, this.previousInput, offset);\n		}\n		readSystemByte(offset) {\n			return systemRegisterByte({\n				frame: this.currentFrame,\n				updates: this.completedUpdates,\n				seconds: this.cartridgeTimeSeconds,\n				rngState: this.rng.state,\n				updateRate: this.updateRate,\n				phase: this.phase,\n				rasterLine: this.rasterLine,\n				booted: this.booted,\n				fault: this.lastFault,\n				used: this.budget.used,\n				limit: this.budget.limit\n			}, offset);\n		}\n		assertRunnable() {\n			if (this.lastFault !== null) throw new RuntimeFault("M019014", "cartridge faulted; restart or restore a healthy checkpoint", this.lastFault.sourceSpan);\n			if (this.phase !== "idle") throw new RuntimeFault("M019014", "cartridge is already executing", {\n				start: 0,\n				end: 0\n			});\n			if (this.currentFrame === Number.MAX_SAFE_INTEGER) this.fault("M019012", "display-frame counter is exhausted", {\n				start: 0,\n				end: 0\n			});\n		}\n		work(units, sourceSpan) {\n			try {\n				this.budget.charge(units, sourceSpan);\n			} catch (error) {\n				this.rememberFault(error, sourceSpan);\n				throw error;\n			}\n		}\n		call(name, arguments_, sourceSpan) {\n			switch (name) {\n				case "rng_num":\n					expectArguments$1(name, arguments_, 0, sourceSpan);\n					return this.rng.nextNum();\n				case "rng_int": {\n					expectArguments$1(name, arguments_, 2, sourceSpan);\n					const minimum = expectInteger(arguments_[0], sourceSpan);\n					const maximum = expectInteger(arguments_[1], sourceSpan);\n					try {\n						return this.rng.nextInt(minimum, maximum);\n					} catch (error) {\n						return this.fault("M019007", error instanceof Error ? error.message : "invalid RNG bounds", sourceSpan);\n					}\n				}\n				case "Vec2":\n					expectArguments$1(name, arguments_, 2, sourceSpan);\n					return {\n						x: expectNumber(arguments_[0], sourceSpan),\n						y: expectNumber(arguments_[1], sourceSpan)\n					};\n				case "Rect":\n					expectArguments$1(name, arguments_, 4, sourceSpan);\n					return {\n						x: expectNumber(arguments_[0], sourceSpan),\n						y: expectNumber(arguments_[1], sourceSpan),\n						w: expectNumber(arguments_[2], sourceSpan),\n						h: expectNumber(arguments_[3], sourceSpan)\n					};\n				case "dither":\n					expectArguments$1(name, arguments_, 5, sourceSpan);\n					return orderedDither(expectInteger(arguments_[0], sourceSpan), expectInteger(arguments_[1], sourceSpan), expectInteger(arguments_[2], sourceSpan), expectInteger(arguments_[3], sourceSpan), expectInteger(arguments_[4], sourceSpan));\n				case "btn":\n				case "btnp": {\n					expectArguments$1(name, arguments_, 2, sourceSpan);\n					const port = expectInteger(arguments_[0], sourceSpan);\n					const button = arguments_[1];\n					if (port < 0 || port >= 4 || !isButton(button)) return this.fault("M019008", "invalid controller port or button", sourceSpan);\n					const pressed = this.input.controllers[port]?.buttons[button] ?? false;\n					if (name === "btn") return pressed;\n					return pressed && !(this.previousInput.controllers[port]?.buttons[button] ?? false);\n				}\n				case "pointer_x":\n					expectArguments$1(name, arguments_, 0, sourceSpan);\n					return this.input.pointer.x;\n				case "pointer_y":\n					expectArguments$1(name, arguments_, 0, sourceSpan);\n					return this.input.pointer.y;\n				case "pointer_inside":\n					expectArguments$1(name, arguments_, 0, sourceSpan);\n					return this.input.pointer.inside;\n				case "pointer_primary":\n				case "pointer_secondary": {\n					expectArguments$1(name, arguments_, 0, sourceSpan);\n					const button = name === "pointer_primary" ? "primary" : "secondary";\n					return this.input.pointer[button] && !this.previousInput.pointer[button];\n				}\n				default: {\n					const context = {\n						frame: this.currentFrame,\n						phase: this.phase,\n						...this.rasterLine === void 0 ? {} : { rasterLine: this.rasterLine }\n					};\n					const result = this.hooks.call?.(name, arguments_, sourceSpan, context);\n					if (this.hooks.call === void 0) return this.fault("M019004", `console API call \'${name}\' is unavailable`, sourceSpan);\n					return result;\n				}\n			}\n		}\n		fault(code, message, sourceSpan) {\n			const error = new RuntimeFault(code, message, sourceSpan);\n			this.rememberFault(error);\n			throw error;\n		}\n		rememberFault(error, fallback = {\n			start: 0,\n			end: 0\n		}) {\n			const span = error instanceof RuntimeFault ? error.sourceSpan : fallback;\n			this.lastFault = {\n				code: error instanceof RuntimeFault && /^M019\\d{3}$/.test(error.code) ? Number(error.code.slice(3)) : 9199,\n				sourceSpan: isFaultSpan(span) ? { ...span } : {\n					start: 0,\n					end: 0\n				}\n			};\n		}\n		probe(id, sourceSpan, locals) {\n			this.hooks.probe?.(id, sourceSpan, locals);\n		}\n		enter(name, sourceSpan) {\n			this.hooks.enter?.(name, sourceSpan);\n		}\n		leave() {\n			this.hooks.leave?.();\n		}\n	};\n	function isMachineSnapshot(value) {\n		if (typeof value !== "object" || value === null || Array.isArray(value)) return false;\n		const candidate = value;\n		if (!(Number.isSafeInteger(candidate.frame) && typeof candidate.frame === "number" && candidate.frame >= 0 && Number.isSafeInteger(candidate.rngState) && isInputFrame(candidate.input) && isInputFrame(candidate.previousInput) && typeof candidate.cartridge === "object" && candidate.cartridge !== null)) return false;\n		if (candidate.revision === 1) return Object.keys(candidate).length === 6;\n		return candidate.revision === 2 && Object.keys(candidate).length === 9 && typeof candidate.rngState === "number" && candidate.rngState > 0 && candidate.rngState <= 4294967295 && (candidate.updateRate === 30 || candidate.updateRate === 60) && isWorkBudgetSnapshot(candidate.budget) && isExecutionSnapshot(candidate.execution, candidate.frame, candidate.updateRate, candidate.budget);\n	}\n	function expectArguments$1(name, arguments_, count, sourceSpan) {\n		if (arguments_.length !== count) throw new RuntimeFault("M019009", `${name} expected ${String(count)} arguments, received ${String(arguments_.length)}`, sourceSpan);\n	}\n	function expectNumber(value, sourceSpan) {\n		if (typeof value !== "number" || !Number.isFinite(value)) throw new RuntimeFault("M019009", "expected a finite number", sourceSpan);\n		return value;\n	}\n	function expectInteger(value, sourceSpan) {\n		const number = expectNumber(value, sourceSpan);\n		if (!Number.isSafeInteger(number)) throw new RuntimeFault("M019009", "expected a safe integer", sourceSpan);\n		return number;\n	}\n	//#endregion\n	//#region src/audio.ts\n	function isSynthSnapshot(value) {\n		if (!audioRecord(value) || Object.keys(value).length !== 5 || value.revision !== 1 || !audioCounter(value.frame) || !audioCounter(value.nextSequence) || value.nextSequence < 1 || !denseAudioArray(value.voices) || value.voices.length !== HARDWARE.audioVoices) return false;\n		const nextSequence = value.nextSequence;\n		if (!value.voices.every((voice, slot) => isVoice(voice, slot, nextSequence))) return false;\n		return value.tracker === null || audioRecord(value.tracker) && Object.keys(value.tracker).length === 4 && typeof value.tracker.music === "string" && value.tracker.music.length > 0 && value.tracker.music.length <= HARDWARE.cartridgeCapacityBytes && audioCounter(value.tracker.orderIndex) && audioCounter(value.tracker.row) && audioCounter(value.tracker.frameInRow);\n	}\n	function isVoice(voice, slot, nextSequence) {\n		return audioRecord(voice) && Object.keys(voice).length === 9 && typeof voice.active === "boolean" && voice.slot === slot && typeof voice.sound === "string" && voice.sound.length <= HARDWARE.cartridgeCapacityBytes && audioNumber(voice.note, 0, 127) && audioNumber(voice.volumeScale, 0, 1) && audioCounter(voice.ageFrames) && audioNumber(voice.phase, 0, 1) && voice.phase < 1 && audioCounter(voice.noiseState) && voice.noiseState > 0 && voice.noiseState <= 4294967295 && audioCounter(voice.sequence) && voice.sequence < nextSequence;\n	}\n	function audioCounter(value) {\n		return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;\n	}\n	function audioNumber(value, minimum, maximum) {\n		return typeof value === "number" && Number.isFinite(value) && value >= minimum && value <= maximum;\n	}\n	function audioRecord(value) {\n		return typeof value === "object" && value !== null && !Array.isArray(value);\n	}\n	/** Validated oscillator and tracker data. Arbitrary PCM samples are intentionally unrepresentable. */\n	var AudioAssetStore = class {\n		entries = /* @__PURE__ */ new Map();\n		ordered;\n		ids = /* @__PURE__ */ new Map();\n		descriptors;\n		constructor(assets = []) {\n			for (const asset of assets) {\n				validateAudioAsset(asset);\n				if (this.entries.has(asset.name)) throw new TypeError(`duplicate audio asset \'${asset.name}\'`);\n				this.entries.set(asset.name, structuredClone(asset));\n			}\n			for (const asset of this.entries.values()) {\n				if (asset.kind !== "music") continue;\n				for (const pattern of Object.values(asset.patterns)) for (const row of pattern.rows) for (const cell of row) if (cell !== null && this.entries.get(cell.sound)?.kind !== "sound") throw new TypeError(`music \'${asset.name}\' references missing sound \'${cell.sound}\'`);\n			}\n			this.ordered = [...this.entries.values()].sort((left, right) => left.name < right.name ? -1 : left.name > right.name ? 1 : 0);\n			this.descriptors = new Uint8Array(this.ordered.length * MEMORY.audioAssetStride);\n			const view = new DataView(this.descriptors.buffer);\n			for (const [id, asset] of this.ordered.entries()) {\n				this.ids.set(asset.name, id);\n				const fields = asset.kind === "sound" ? [\n					1,\n					[\n						"pulse",\n						"triangle",\n						"saw",\n						"noise",\n						"wavetable"\n					].indexOf(asset.waveform) + 1,\n					asset.note,\n					asset.durationFrames,\n					asset.envelope.releaseFrames,\n					0,\n					0,\n					0\n				] : [\n					2,\n					0,\n					0,\n					0,\n					0,\n					asset.framesPerRow,\n					asset.order.length,\n					Number(asset.loop)\n				];\n				for (const [field, value] of fields.entries()) view.setUint32(id * MEMORY.audioAssetStride + field * 4, value, true);\n			}\n		}\n		get(name) {\n			return this.entries.get(name);\n		}\n		id(name) {\n			return this.ids.get(name) ?? -1;\n		}\n		byId(id) {\n			return this.ordered[id];\n		}\n		get count() {\n			return this.ordered.length;\n		}\n		memoryRegions() {\n			return this.descriptors.length === 0 ? [] : [{\n				name: "audio asset descriptors",\n				address: MEMORY.audioAssets,\n				bytes: this.descriptors,\n				writable: false\n			}];\n		}\n	};\n	/** Eight-voice deterministic synthesizer and eight-channel order/pattern tracker. */\n	var Synthesizer = class {\n		assets;\n		voices;\n		frame = 0;\n		nextSequence = 1;\n		tracker = null;\n		constructor(assets = new AudioAssetStore()) {\n			this.assets = assets;\n			this.voices = Array.from({ length: HARDWARE.audioVoices }, (_, slot) => emptyVoice(slot));\n		}\n		executeFrame(commands) {\n			for (const command of commands) this.executeCommand(command);\n			return this.finishFrame();\n		}\n		finishFrame() {\n			if (this.frame === Number.MAX_SAFE_INTEGER) throw new RuntimeFault("M019012", "audio-frame counter is exhausted", {\n				start: 0,\n				end: 0\n			});\n			this.advanceTracker();\n			const sampleCount = HARDWARE.audioSampleRate / HARDWARE.frameRate;\n			const left = new Float32Array(sampleCount);\n			const right = new Float32Array(sampleCount);\n			for (const voice of this.voices) if (voice.active) this.mixVoice(voice, left, right);\n			for (const voice of this.voices) if (voice.active) {\n				voice.ageFrames += 1;\n				const sound = this.assets.get(voice.sound);\n				if (sound?.kind !== "sound" || voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames) voice.active = false;\n			}\n			const inspection = this.inspectTracker();\n			this.frame += 1;\n			return {\n				left,\n				right,\n				activeVoices: this.voices.filter((voice) => voice.active).length,\n				tracker: inspection\n			};\n		}\n		snapshot() {\n			return structuredClone({\n				revision: 1,\n				frame: this.frame,\n				nextSequence: this.nextSequence,\n				voices: this.voices,\n				tracker: this.tracker\n			});\n		}\n		restore(snapshot) {\n			if (!isSynthSnapshot(snapshot)) throw new TypeError("invalid MOD-01 synthesizer snapshot");\n			for (const voice of snapshot.voices) {\n				const sound = this.assets.get(voice.sound);\n				if ((voice.active || voice.sound !== "") && sound?.kind !== "sound") throw new TypeError("snapshot references a missing sound");\n				if (voice.active && sound?.kind === "sound" && voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames) throw new TypeError("snapshot contains a voice beyond its sound lifetime");\n			}\n			if (snapshot.tracker !== null) {\n				const tracker = snapshot.tracker;\n				const music = this.assets.get(tracker.music);\n				const patternName = music?.kind === "music" ? music.order[tracker.orderIndex] : void 0;\n				if (music?.kind !== "music" || patternName === void 0 || music.patterns[patternName]?.rows[tracker.row] === void 0 || tracker.frameInRow >= music.framesPerRow) throw new TypeError("snapshot references an invalid tracker position");\n			}\n			this.frame = snapshot.frame;\n			this.nextSequence = snapshot.nextSequence;\n			for (let index = 0; index < this.voices.length; index += 1) Object.assign(this.voices[index], structuredClone(snapshot.voices[index]));\n			this.tracker = structuredClone(snapshot.tracker);\n		}\n		inspectVoices() {\n			return structuredClone(this.voices);\n		}\n		inspectTracker() {\n			return this.tracker === null ? null : { ...this.tracker };\n		}\n		memoryRegions() {\n			return [\n				...this.assets.memoryRegions(),\n				{\n					name: "synth status",\n					address: MEMORY.audio,\n					length: 32,\n					writable: false,\n					readByte: (offset) => this.statusByte(offset)\n				},\n				{\n					name: "tracker controls",\n					address: MEMORY.audioTracker,\n					length: 16,\n					writable: true,\n					readByte: (offset) => this.trackerByte(offset),\n					prepareWrite: (offset, bytes) => this.prepareTrackerWrite(offset, bytes)\n				},\n				...this.voices.flatMap((voice) => [{\n					name: `voice ${String(voice.slot)} controls`,\n					address: MEMORY.audioVoices + voice.slot * MEMORY.voiceStride,\n					length: 48,\n					writable: true,\n					readByte: (offset) => this.voiceByte(voice, offset),\n					prepareWrite: (offset, bytes) => this.prepareVoiceWrite(voice, offset, bytes)\n				}, {\n					name: `voice ${String(voice.slot)} allocation`,\n					address: MEMORY.audioVoices + voice.slot * MEMORY.voiceStride + 48,\n					length: 16,\n					writable: false,\n					readByte: (offset) => offset < 8 ? integerByte(voice.sequence, offset) : 0\n				}])\n			];\n		}\n		statusByte(offset) {\n			if (offset < 8) return integerByte(this.frame, offset);\n			if (offset < 16) return integerByte(this.nextSequence, offset - 8);\n			if (offset === 16) return this.voices.filter((voice) => voice.active).length;\n			if (offset === 17) return HARDWARE.audioVoices;\n			if (offset === 18) return HARDWARE.trackerChannels;\n			if (offset === 19) return Number(this.tracker !== null);\n			if (offset < 24) return integerByte(HARDWARE.audioSampleRate, offset - 20);\n			if (offset < 28) return integerByte(this.assets.count, offset - 24);\n			return integerByte(MEMORY.audioAssets, offset - 28);\n		}\n		trackerByte(offset) {\n			if (this.tracker === null) return 0;\n			return integerByte([\n				this.assets.id(this.tracker.music) + 1,\n				this.tracker.orderIndex,\n				this.tracker.row,\n				this.tracker.frameInRow\n			][Math.floor(offset / 4)] ?? 0, offset % 4);\n		}\n		prepareTrackerWrite(offset, bytes) {\n			const staged = Uint8Array.from({ length: 16 }, (_, index) => this.trackerByte(index));\n			staged.set(bytes, offset);\n			const view = new DataView(staged.buffer);\n			const id = view.getUint32(0, true);\n			if (id === 0) return () => {\n				this.tracker = null;\n			};\n			const music = this.assets.byId(id - 1);\n			if (music?.kind !== "music") return void 0;\n			const next = {\n				music: music.name,\n				orderIndex: view.getUint32(4, true),\n				row: view.getUint32(8, true),\n				frameInRow: view.getUint32(12, true)\n			};\n			const patternName = music.order[next.orderIndex];\n			if (patternName === void 0 || music.patterns[patternName]?.rows[next.row] === void 0 || next.frameInRow >= music.framesPerRow) return void 0;\n			return () => {\n				this.tracker = next;\n			};\n		}\n		voiceByte(voice, offset) {\n			if (offset === 0) return Number(voice.active);\n			if (offset < 4) return 0;\n			if (offset < 8) return integerByte(this.assets.id(voice.sound) + 1, offset - 4);\n			if (offset < 16) return floatByte(voice.note, offset - 8);\n			if (offset < 24) return floatByte(voice.volumeScale, offset - 16);\n			if (offset < 32) return integerByte(voice.ageFrames, offset - 24);\n			if (offset < 40) return floatByte(voice.phase, offset - 32);\n			if (offset < 44) return integerByte(voice.noiseState, offset - 40);\n			return 0;\n		}\n		prepareVoiceWrite(voice, offset, bytes) {\n			const staged = Uint8Array.from({ length: 48 }, (_, index) => this.voiceByte(voice, index));\n			staged.set(bytes, offset);\n			if (staged[0] !== 0 && staged[0] !== 1) return void 0;\n			if ([\n				1,\n				2,\n				3,\n				44,\n				45,\n				46,\n				47\n			].some((index) => staged[index] !== 0)) return void 0;\n			const view = new DataView(staged.buffer);\n			const id = view.getUint32(4, true);\n			const sound = this.assets.byId(id - 1);\n			if (id !== 0 && sound?.kind !== "sound") return void 0;\n			const next = {\n				...voice,\n				active: staged[0] === 1,\n				sound: sound?.name ?? "",\n				note: view.getFloat64(8, true),\n				volumeScale: view.getFloat64(16, true),\n				ageFrames: Number(view.getBigUint64(24, true)),\n				phase: view.getFloat64(32, true),\n				noiseState: view.getUint32(40, true)\n			};\n			if (!isVoice(next, voice.slot, this.nextSequence) || next.active && (sound?.kind !== "sound" || next.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames)) return void 0;\n			return () => {\n				Object.assign(voice, next);\n			};\n		}\n		executeCommand(command) {\n			switch (command.name) {\n				case "sfx": {\n					const [handle] = expectArguments(command, 1);\n					this.trigger(readAssetName(handle, "Sound"), void 0, 1, command.sourceSpan);\n					return;\n				}\n				case "music": {\n					const [handle] = expectArguments(command, 1);\n					const name = readAssetName(handle, "Music");\n					if (this.assets.get(name)?.kind !== "music") throw new TypeError(`missing Music asset \'${name}\'`);\n					this.tracker = {\n						music: name,\n						orderIndex: 0,\n						row: 0,\n						frameInRow: 0\n					};\n					return;\n				}\n				case "music_stop":\n					expectArguments(command, 0);\n					this.tracker = null;\n					return;\n				default: throw new TypeError(`unknown audio command \'${command.name}\'`);\n			}\n		}\n		trigger(soundName, noteOverride, volumeScale = 1, sourceSpan = {\n			start: 0,\n			end: 0\n		}) {\n			const sound = this.assets.get(soundName);\n			if (sound?.kind !== "sound") throw new TypeError(`missing Sound asset \'${soundName}\'`);\n			if (this.nextSequence === Number.MAX_SAFE_INTEGER) throw new RuntimeFault("M019012", "voice-allocation counter is exhausted", sourceSpan);\n			const voice = this.voices.find((candidate) => !candidate.active) ?? this.voices.reduce((oldest, candidate) => candidate.sequence < oldest.sequence ? candidate : oldest);\n			Object.assign(voice, {\n				active: true,\n				sound: soundName,\n				note: noteOverride ?? sound.note,\n				volumeScale,\n				ageFrames: 0,\n				phase: 0,\n				noiseState: nonZeroNoiseSeed(this.frame, voice.slot, this.nextSequence),\n				sequence: this.nextSequence\n			});\n			this.nextSequence += 1;\n		}\n		advanceTracker() {\n			const tracker = this.tracker;\n			if (tracker === null) return;\n			const music = this.assets.get(tracker.music);\n			if (music?.kind !== "music") {\n				this.tracker = null;\n				return;\n			}\n			if (tracker.frameInRow === 0) {\n				const patternName = music.order[tracker.orderIndex];\n				((patternName === void 0 ? void 0 : music.patterns[patternName])?.rows[tracker.row])?.forEach((cell) => {\n					if (cell !== null) this.trigger(cell.sound, cell.note, cell.volume ?? 1);\n				});\n			}\n			tracker.frameInRow += 1;\n			if (tracker.frameInRow < music.framesPerRow) return;\n			tracker.frameInRow = 0;\n			tracker.row += 1;\n			const patternName = music.order[tracker.orderIndex];\n			const pattern = patternName === void 0 ? void 0 : music.patterns[patternName];\n			if (pattern !== void 0 && tracker.row < pattern.rows.length) return;\n			tracker.row = 0;\n			tracker.orderIndex += 1;\n			if (tracker.orderIndex >= music.order.length) {\n				if (music.loop) tracker.orderIndex = 0;\n				else this.tracker = null;\n			}\n		}\n		mixVoice(voice, left, right) {\n			const sound = this.assets.get(voice.sound);\n			if (sound?.kind !== "sound") {\n				voice.active = false;\n				return;\n			}\n			const frameEnvelope = envelopeAt(sound, voice.ageFrames);\n			const pan = Math.max(-1, Math.min(1, sound.pan));\n			const leftGain = (1 - pan) / 2;\n			const rightGain = (1 + pan) / 2;\n			const phaseStep = noteFrequency(voice.note + sound.pitch.slideSemitonesPerFrame * voice.ageFrames + triangleLfo(voice.ageFrames, sound.pitch.vibratoPeriodFrames) * sound.pitch.vibratoDepthSemitones) / HARDWARE.audioSampleRate;\n			for (let index = 0; index < left.length; index += 1) {\n				const sample = oscillatorValue(sound, voice) * frameEnvelope * sound.volume * voice.volumeScale * .24;\n				left[index] = clampSample((left[index] ?? 0) + sample * leftGain);\n				right[index] = clampSample((right[index] ?? 0) + sample * rightGain);\n				voice.phase = (voice.phase + phaseStep) % 1;\n			}\n		}\n	};\n	function emptyVoice(slot) {\n		return {\n			active: false,\n			slot,\n			sound: "",\n			note: 0,\n			volumeScale: 1,\n			ageFrames: 0,\n			phase: 0,\n			noiseState: 1,\n			sequence: 0\n		};\n	}\n	function integerByte(value, offset) {\n		return Math.floor(value / 2 ** (offset * 8)) & 255;\n	}\n	function floatByte(value, offset) {\n		const view = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));\n		view.setFloat64(0, value, true);\n		return view.getUint8(offset);\n	}\n	function validateAudioAsset(asset) {\n		if (!audioRecord(asset) || typeof asset.name !== "string" || asset.name.length === 0) throw new TypeError("audio assets require a nonempty name");\n		if (asset.kind === "sound") {\n			if (typeof asset.waveform !== "string" || ![\n				"pulse",\n				"triangle",\n				"saw",\n				"noise",\n				"wavetable"\n			].includes(asset.waveform) || !audioInteger(asset.note, 0, 127) || !audioInteger(asset.durationFrames, 1, 3600) || !audioNumber(asset.volume, 0, 1) || !audioNumber(asset.pan, -1, 1) || !validEnvelope(asset.envelope) || !validPitch(asset.pitch, asset.durationFrames + asset.envelope.releaseFrames)) throw new RangeError(`sound \'${asset.name}\' is outside MOD-01 limits`);\n			if (asset.waveform === "pulse" && (!audioNumber(asset.duty, 0, 1) || asset.duty === 0 || asset.duty === 1)) throw new RangeError(`pulse sound \'${asset.name}\' requires a duty between zero and one`);\n			if (asset.waveform === "wavetable" && (!denseAudioArray(asset.wavetable) || asset.wavetable.length < 4 || asset.wavetable.length > 32 || asset.wavetable.some((sample) => !audioNumber(sample, -1, 1)))) throw new RangeError(`wavetable sound \'${asset.name}\' requires 4-32 normalized entries`);\n			return;\n		}\n		if (asset.kind !== "music" || typeof asset.loop !== "boolean" || !audioInteger(asset.framesPerRow, 1, 240) || !denseAudioArray(asset.order) || asset.order.length === 0 || !audioRecord(asset.patterns) || asset.order.some((name) => typeof name !== "string" || !Object.hasOwn(asset.patterns, name))) throw new RangeError(`music \'${asset.name}\' has an invalid order list or tempo`);\n		for (const pattern of Object.values(asset.patterns)) {\n			if (!audioRecord(pattern) || !denseAudioArray(pattern.rows) || pattern.rows.length === 0 || pattern.rows.length > 256) throw new RangeError(`music \'${asset.name}\' has an invalid pattern length`);\n			for (const row of pattern.rows) {\n				if (!denseAudioArray(row) || row.length !== HARDWARE.trackerChannels) throw new RangeError(`music \'${asset.name}\' patterns must have eight channels`);\n				for (const cell of row) if (cell !== null && (!audioRecord(cell) || !audioInteger(cell.note, 0, 127) || typeof cell.sound !== "string" || cell.sound.length === 0 || cell.volume !== void 0 && !audioNumber(cell.volume, 0, 1))) throw new RangeError(`music \'${asset.name}\' contains an invalid note`);\n			}\n		}\n	}\n	function denseAudioArray(value) {\n		return Array.isArray(value) && Object.keys(value).length === value.length && Array.from(value.keys()).every((index) => Object.hasOwn(value, index));\n	}\n	function audioInteger(value, minimum, maximum) {\n		return audioNumber(value, minimum, maximum) && Number.isSafeInteger(value);\n	}\n	function validEnvelope(envelope) {\n		return audioRecord(envelope) && [\n			envelope.attackFrames,\n			envelope.decayFrames,\n			envelope.releaseFrames\n		].every((value) => audioInteger(value, 0, 3600)) && audioNumber(envelope.sustainLevel, 0, 1);\n	}\n	function validPitch(pitch, lifetime) {\n		if (!audioRecord(pitch) || typeof pitch.slideSemitonesPerFrame !== "number" || !Number.isFinite(pitch.slideSemitonesPerFrame) || typeof pitch.vibratoDepthSemitones !== "number" || !Number.isFinite(pitch.vibratoDepthSemitones) || !audioInteger(pitch.vibratoPeriodFrames, 0, 3600)) return false;\n		const excursion = Math.abs(pitch.slideSemitonesPerFrame) * lifetime + Math.abs(pitch.vibratoDepthSemitones);\n		return Number.isFinite(excursion) && Number.isFinite(noteFrequency(127 + excursion));\n	}\n	function oscillatorValue(sound, voice) {\n		switch (sound.waveform) {\n			case "pulse": return voice.phase < (sound.duty ?? .5) ? 1 : -1;\n			case "triangle": return 1 - 4 * Math.abs(voice.phase - .5);\n			case "saw": return voice.phase * 2 - 1;\n			case "noise":\n				voice.noiseState = xorshift(voice.noiseState);\n				return (voice.noiseState & 65535) / 32767 - 1;\n			case "wavetable": {\n				const table = sound.wavetable ?? [0];\n				return table[Math.floor(voice.phase * table.length) % table.length] ?? 0;\n			}\n		}\n	}\n	function envelopeAt(sound, age) {\n		const envelope = sound.envelope;\n		if (envelope.attackFrames > 0 && age < envelope.attackFrames) return age / envelope.attackFrames;\n		const decayAge = age - envelope.attackFrames;\n		if (envelope.decayFrames > 0 && decayAge < envelope.decayFrames) return 1 - (1 - envelope.sustainLevel) * (decayAge / envelope.decayFrames);\n		if (age < sound.durationFrames) return envelope.sustainLevel;\n		if (envelope.releaseFrames === 0) return 0;\n		return envelope.sustainLevel * Math.max(0, 1 - (age - sound.durationFrames) / envelope.releaseFrames);\n	}\n	function triangleLfo(frame, period) {\n		if (period === 0) return 0;\n		const phase = frame % period / period;\n		return 1 - 4 * Math.abs(phase - .5);\n	}\n	function noteFrequency(note) {\n		return 440 * 2 ** ((note - 69) / 12);\n	}\n	function xorshift(state) {\n		let value = state >>> 0;\n		value ^= value << 13;\n		value ^= value >>> 17;\n		value ^= value << 5;\n		return value >>> 0;\n	}\n	function nonZeroNoiseSeed(frame, slot, sequence) {\n		return (frame + 1) * 2654435761 + (slot + 1) * 2246822507 + sequence >>> 0 || 1;\n	}\n	function clampSample(sample) {\n		return Math.max(-1, Math.min(1, sample));\n	}\n	function expectArguments(command, count) {\n		if (command.arguments.length !== count) throw new TypeError(`${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`);\n		return [...command.arguments];\n	}\n	function readAssetName(value, kind) {\n		if (typeof value !== "object" || value === null || !("name" in value) || typeof value.name !== "string" || !("kind" in value) || value.kind !== kind) throw new TypeError(`expected a ${kind} asset handle`);\n		return value.name;\n	}\n	//#endregion\n	//#region src/asset-codec.ts\n	/** Bounded, data-only source bank sent to the restricted Worker for authoritative decoding. */\n	function isRuntimeAssetSource(value) {\n		if (!isRecord$5(value) || !isRecord$5(value.declarations) || !isRecord$5(value.files) || Array.isArray(value.declarations) || Array.isArray(value.files) || Object.keys(value).some((key) => ![\n			"declarations",\n			"files",\n			"displayPath"\n		].includes(key)) || Object.keys(value.declarations).length > 4096 || Object.keys(value.files).length > 4096 || value.displayPath !== void 0 && value.displayPath !== null && !canonicalAssetPath(value.displayPath)) return false;\n		let bytes = typeof value.displayPath === "string" ? value.displayPath.length : 0;\n		for (const [path, data] of Object.entries(value.files)) {\n			if (!canonicalAssetPath(path) || !(data instanceof Uint8Array)) return false;\n			bytes += path.length + data.byteLength;\n			if (bytes > 16777216) return false;\n		}\n		for (const [name, declaration] of Object.entries(value.declarations)) {\n			if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) || !isRecord$5(declaration) || Object.keys(declaration).length !== 2 || !canonicalAssetPath(declaration.path) || typeof declaration.kind !== "string" || ![\n				"sprite",\n				"animation",\n				"tile_set",\n				"map",\n				"font",\n				"sound",\n				"music"\n			].includes(declaration.kind)) return false;\n			bytes += name.length + declaration.path.length + declaration.kind.length;\n			if (bytes > 16777216) return false;\n		}\n		return true;\n	}\n	function canonicalAssetPath(value) {\n		return typeof value === "string" && value.length > 0 && value.length <= 1024 && value.split("/").every((part) => /^[A-Za-z0-9_.-]+$/.test(part) && part !== "." && part !== "..");\n	}\n	/** Decodes documented JSON asset files into validated hardware stores and worker map views. */\n	function decodeRuntimeAssets(declarations, files, displayPath) {\n		const visual = [];\n		const audio = [];\n		for (const [name, declaration] of Object.entries(declarations).sort(([left], [right]) => left.localeCompare(right))) {\n			const bytes = files[declaration.path];\n			if (bytes === void 0) throw new TypeError(`asset \'${name}\' is missing \'${declaration.path}\'`);\n			const value = JSON.parse(new TextDecoder().decode(bytes));\n			switch (declaration.kind) {\n				case "sprite":\n					visual.push(decodeSprite(name, value, false));\n					break;\n				case "animation":\n					visual.push(decodeSprite(name, value, true));\n					break;\n				case "tile_set":\n					visual.push(decodeTileSet(name, value));\n					break;\n				case "map":\n					visual.push(decodeMap(name, value));\n					break;\n				case "sound":\n					audio.push(decodeSound(name, value));\n					break;\n				case "music":\n					audio.push(decodeMusic(name, value));\n					break;\n				case "font": visual.push(decodeFont(name, value));\n			}\n		}\n		const visualStore = new VisualAssetStore(visual);\n		new AudioAssetStore(audio);\n		const tileSets = new Map(visual.filter((asset) => asset.kind === "tile_set").map((asset) => [asset.name, asset]));\n		const maps = visual.filter((asset) => asset.kind === "map").map((asset) => ({\n			name: asset.name,\n			layers: asset.layers.map((layer) => {\n				const tileSet = tileSets.get(layer.tileSet);\n				if (tileSet === void 0) throw new TypeError(`map \'${asset.name}\' references missing tile set \'${layer.tileSet}\'`);\n				return {\n					width: layer.width,\n					height: layer.height,\n					cells: layer.cells.slice(),\n					tileFlags: tileSet.flags.slice()\n				};\n			})\n		}));\n		const display = decodeDisplay(displayPath, files);\n		const visualBytes = visualStore.usedBytes + displayBytes(display);\n		if (visualBytes > HARDWARE.visualCapacityBytes) throw new RangeError("visual assets exceed the 1 MiB shared capacity");\n		return {\n			visual,\n			audio,\n			maps,\n			...display === void 0 ? {} : { display },\n			visualBytes\n		};\n	}\n	function decodeSprite(name, value, animation) {\n		if (!isRecord$5(value) || value.revision !== 1 && value.revision !== 2 || value.kind !== "sprite" || !boundedInteger(value.width, 1, value.revision === 1 ? 64 : HARDWARE.spriteMaximumAxis) || !boundedInteger(value.height, 1, value.revision === 1 ? 64 : HARDWARE.spriteMaximumAxis) || !Array.isArray(value.frames) || value.frames.length === 0 || value.frames.length > 256) throw new TypeError(`sprite asset \'${name}\' is invalid`);\n		const pixelCount = value.width * value.height;\n		const frames = value.frames.map((frame) => {\n			if (!isNumberArray(frame, pixelCount, 0, value.revision === 1 ? 31 : 255)) throw new TypeError(`sprite asset \'${name}\' has invalid indexed pixels`);\n			return {\n				kind: "sprite",\n				name,\n				width: value.width,\n				height: value.height,\n				pixels: Uint8Array.from(frame)\n			};\n		});\n		const first = frames[0];\n		if (first === void 0) throw new TypeError(`sprite asset \'${name}\' requires a frame`);\n		return animation ? {\n			kind: "animation",\n			name,\n			frames\n		} : {\n			...first,\n			name\n		};\n	}\n	function decodeTileSet(name, value) {\n		if (!isRecord$5(value) || value.revision !== 1 && value.revision !== 2 || value.kind !== "tile_set" || !Array.isArray(value.tiles) || value.tiles.length === 0 || value.tiles.length > 4096 || !isNumberArray(value.flags, value.tiles.length, 0, 255)) throw new TypeError(`tile-set asset \'${name}\' is invalid`);\n		return {\n			kind: "tile_set",\n			name,\n			tiles: value.tiles.map((pixels, index) => {\n				if (!isNumberArray(pixels, 64, 0, value.revision === 1 ? 31 : 255)) throw new TypeError(`tile ${String(index)} in \'${name}\' has invalid indexed pixels`);\n				return {\n					kind: "sprite",\n					name: `${name}:${String(index)}`,\n					width: 8,\n					height: 8,\n					pixels: Uint8Array.from(pixels)\n				};\n			}),\n			flags: Uint8Array.from(value.flags)\n		};\n	}\n	function decodeMap(name, value) {\n		if (!isRecord$5(value) || value.revision !== 1 || value.kind !== "map" || !Array.isArray(value.layers) || value.layers.length === 0 || value.layers.length > 8) throw new TypeError(`map asset \'${name}\' is invalid`);\n		return {\n			kind: "map",\n			name,\n			layers: value.layers.map((layer) => {\n				if (!isRecord$5(layer) || !boundedInteger(layer.width, 1, 256) || !boundedInteger(layer.height, 1, 256) || typeof layer.tileSet !== "string" || !isNumberArray(layer.cells, layer.width * layer.height, 0, 65535)) throw new TypeError(`map asset \'${name}\' has an invalid layer`);\n				return {\n					width: layer.width,\n					height: layer.height,\n					cells: Uint16Array.from(layer.cells),\n					tileSet: layer.tileSet\n				};\n			})\n		};\n	}\n	function decodeFont(name, value) {\n		if (!isRecord$5(value) || value.revision !== 1 || value.kind !== "font" || !boundedInteger(value.glyphWidth, 1, 16) || !boundedInteger(value.glyphHeight, 1, 16) || !boundedInteger(value.baseline, 0, value.glyphHeight - 1) || !boundedInteger(value.advanceX, 1, 32) || !boundedInteger(value.advanceY, 1, 32) || !boundedInteger(value.missingGlyph, 0, 255) || !Array.isArray(value.glyphs) || value.glyphs.length === 0 || value.glyphs.length > 256) throw new TypeError(`font asset \'${name}\' is invalid`);\n		const glyphs = /* @__PURE__ */ new Map();\n		let previous = -1;\n		for (const glyph of value.glyphs) {\n			if (!isRecord$5(glyph) || !boundedInteger(glyph.code, 0, 255) || glyph.code <= previous || !isNumberArray(glyph.pixels, value.glyphWidth * value.glyphHeight, 0, 1)) throw new TypeError(`font asset \'${name}\' has an invalid glyph map`);\n			previous = glyph.code;\n			glyphs.set(glyph.code, Uint8Array.from(glyph.pixels));\n		}\n		if (!glyphs.has(value.missingGlyph)) throw new TypeError(`font asset \'${name}\' is missing its fallback glyph`);\n		return {\n			kind: "font",\n			name,\n			glyphWidth: value.glyphWidth,\n			glyphHeight: value.glyphHeight,\n			baseline: value.baseline,\n			advanceX: value.advanceX,\n			advanceY: value.advanceY,\n			missingGlyph: value.missingGlyph,\n			glyphs\n		};\n	}\n	function decodeSound(name, value) {\n		if (!isRecord$5(value) || value.revision !== 1 || value.kind !== "sound") throw new TypeError(`sound asset \'${name}\' is invalid`);\n		const sound = {\n			...value,\n			name\n		};\n		new AudioAssetStore([sound]);\n		return sound;\n	}\n	function decodeMusic(name, value) {\n		if (!isRecord$5(value) || value.revision !== 1 || value.kind !== "music") throw new TypeError(`music asset \'${name}\' is invalid`);\n		return {\n			...value,\n			name\n		};\n	}\n	function decodeDisplay(path, files) {\n		if (path === void 0 || path === null) return void 0;\n		const bytes = files[path];\n		if (bytes === void 0) throw new TypeError(`display configuration is missing \'${path}\'`);\n		const value = JSON.parse(new TextDecoder().decode(bytes));\n		if (!isRecord$5(value) || value.revision !== 1 && value.revision !== 2 || value.kind !== "display" || !isNumberArray(value.remap, HARDWARE.rasterPaletteSize, 0, value.revision === 1 ? 31 : 255) || !Array.isArray(value.raster) || value.raster.length > HARDWARE.height) throw new TypeError("display configuration is invalid");\n		let previousLine = -1;\n		const raster = value.raster.map((state) => {\n			if (!isRecord$5(state) || !boundedInteger(state.line, 0, HARDWARE.height - 1) || state.line <= previousLine || !boundedInteger(state.scrollX, -32768, 32767) || !boundedInteger(state.scrollY, -32768, 32767) || !isNumberArray(state.remap, HARDWARE.rasterPaletteSize, 0, value.revision === 1 ? 31 : 255)) throw new TypeError("display configuration has invalid raster state");\n			previousLine = state.line;\n			return {\n				line: state.line,\n				scrollX: state.scrollX,\n				scrollY: state.scrollY,\n				remap: Uint8Array.from(state.remap)\n			};\n		});\n		return {\n			remap: Uint8Array.from(value.remap),\n			raster\n		};\n	}\n	function displayBytes(display) {\n		return display === void 0 ? 0 : HARDWARE.rasterPaletteSize + display.raster.length * (HARDWARE.rasterPaletteSize + 6);\n	}\n	function isNumberArray(value, length, minimum, maximum) {\n		return Array.isArray(value) && value.length === length && value.every((entry) => Number.isSafeInteger(entry) && entry >= minimum && entry <= maximum);\n	}\n	function boundedInteger(value, minimum, maximum) {\n		return Number.isSafeInteger(value) && typeof value === "number" && value >= minimum && value <= maximum;\n	}\n	function isRecord$5(value) {\n		return typeof value === "object" && value !== null;\n	}\n	//#endregion\n	//#region src/map-query.ts\n	/** Worker-safe, read-only map view exposed to cartridge query calls. */\n	var MapQueryStore = class {\n		maps = /* @__PURE__ */ new Map();\n		constructor(assets = []) {\n			for (const asset of assets) {\n				if (asset.name.length === 0 || this.maps.has(asset.name) || asset.layers.length === 0) throw new TypeError("map query catalog contains an invalid or duplicate map");\n				for (const layer of asset.layers) if (!Number.isSafeInteger(layer.width) || !Number.isSafeInteger(layer.height) || layer.width <= 0 || layer.height <= 0 || layer.cells.length !== layer.width * layer.height || layer.cells.some((tile) => tile >= layer.tileFlags.length)) throw new TypeError(`map query data for \'${asset.name}\' is incoherent`);\n				this.maps.set(asset.name, asset);\n			}\n		}\n		cell(name, layerIndex, x, y) {\n			const layer = this.maps.get(name)?.layers[layerIndex];\n			if (layer === void 0 || x < 0 || y < 0 || x >= layer.width || y >= layer.height) return -1;\n			return layer.cells[y * layer.width + x] ?? -1;\n		}\n		flag(name, layerIndex, x, y, flagIndex) {\n			if (flagIndex < 0 || flagIndex > 7) return false;\n			const layer = this.maps.get(name)?.layers[layerIndex];\n			const tile = this.cell(name, layerIndex, x, y);\n			return layer !== void 0 && tile >= 0 && ((layer.tileFlags[tile] ?? 0) & 1 << flagIndex) !== 0;\n		}\n	};\n	function isMapQueryCatalog(value) {\n		return Array.isArray(value) && value.every(isMapQueryAsset);\n	}\n	function isMapQueryAsset(value) {\n		return isRecord$4(value) && typeof value.name === "string" && value.name.length > 0 && Array.isArray(value.layers) && value.layers.length > 0 && value.layers.every(isMapQueryLayer);\n	}\n	function isMapQueryLayer(value) {\n		if (!isRecord$4(value) || !Number.isSafeInteger(value.width) || typeof value.width !== "number" || value.width <= 0 || !Number.isSafeInteger(value.height) || typeof value.height !== "number" || value.height <= 0 || !(value.cells instanceof Uint16Array) || value.cells.length !== value.width * value.height || !(value.tileFlags instanceof Uint8Array)) return false;\n		const tileFlags = value.tileFlags;\n		return value.cells.every((tile) => tile < tileFlags.length);\n	}\n	function isRecord$4(value) {\n		return typeof value === "object" && value !== null;\n	}\n	//#endregion\n	//#region src/save.ts\n	/** One byte image, with a commit latch delivered to the trusted host after a successful frame. */\n	var SaveMemory = class SaveMemory {\n		bytes = new Uint8Array(HARDWARE.saveCapacityBytes);\n		committed = new Uint8Array(HARDWARE.saveCapacityBytes);\n		values;\n		pendingCommit = false;\n		commits = 0;\n		dirtyBytes = 0;\n		writes = /* @__PURE__ */ new Map();\n		constructor(initial = {}) {\n			if (!isSaveImage(initial)) throw new TypeError("invalid MOD-01 save image");\n			this.bytes.set(initial instanceof Uint8Array ? initial : encodeValues(initial));\n			this.committed.set(this.bytes);\n		}\n		get(key, fallback) {\n			validateKey(key);\n			validateInteger(fallback);\n			const values = this.readValues();\n			return Object.hasOwn(values, key) ? values[key] ?? fallback : fallback;\n		}\n		set(key, value) {\n			validateKey(key);\n			validateInteger(value);\n			this.checkCommitCounter();\n			const next = {\n				...this.readValues(),\n				[key]: value\n			};\n			const bytes = encodeValues(next);\n			this.bytes.fill(0);\n			this.bytes.set(bytes);\n			this.values = next;\n			this.latch();\n			for (const [pendingKey, pendingValue] of this.writes) if (!Object.hasOwn(next, pendingKey) || next[pendingKey] !== pendingValue) this.writes.delete(pendingKey);\n			this.writes.set(key, value);\n		}\n		snapshot() {\n			return sortedValues(this.readValues());\n		}\n		deviceSnapshot() {\n			return {\n				revision: 1,\n				bytes: this.bytes.slice(),\n				committed: this.committed.slice(),\n				pendingCommit: this.pendingCommit,\n				commits: this.commits\n			};\n		}\n		restoreDevice(value, pendingWrites = []) {\n			if (!isSaveSnapshot(value) || !isPendingDeviceWrites(pendingWrites, value)) throw new TypeError("invalid MOD-01 save snapshot");\n			this.bytes.set(value.bytes);\n			this.committed.set(value.committed);\n			this.values = void 0;\n			this.pendingCommit = value.pendingCommit;\n			this.commits = value.commits;\n			this.dirtyBytes = this.bytes.reduce((count, byte, index) => count + Number(byte !== this.committed[index]), 0);\n			this.writes.clear();\n			for (const write of pendingWrites) this.writes.set(write.key, write.value);\n		}\n		commit() {\n			this.checkCommitCounter();\n			this.latch();\n			this.writes.clear();\n		}\n		takeCommit() {\n			if (!this.pendingCommit) return void 0;\n			this.pendingCommit = false;\n			this.writes.clear();\n			return this.committed.slice();\n		}\n		restore(value, pendingWrites = []) {\n			if (!isSaveValues(value) || !isPendingSaveWrites(pendingWrites, value)) throw new TypeError("invalid MOD-01 save snapshot");\n			const initial = new SaveMemory(value).deviceSnapshot();\n			this.restoreDevice({\n				...initial,\n				pendingCommit: pendingWrites.length > 0\n			}, pendingWrites);\n		}\n		pendingWrites() {\n			return [...this.writes].sort(([left], [right]) => left.localeCompare(right)).map(([key, value]) => ({\n				key,\n				value\n			}));\n		}\n		takeWrites() {\n			const writes = this.pendingWrites();\n			this.writes.clear();\n			return writes;\n		}\n		memoryRegions(charge) {\n			return [\n				{\n					name: "save working bytes",\n					address: MEMORY.save,\n					length: this.bytes.length,\n					writable: true,\n					readByte: (offset) => this.bytes[offset] ?? 0,\n					prepareWrite: (offset, bytes) => () => {\n						for (let index = 0; index < bytes.length; index += 1) {\n							const cursor = offset + index;\n							this.dirtyBytes += Number(bytes[index] !== this.committed[cursor]) - Number(this.bytes[cursor] !== this.committed[cursor]);\n						}\n						this.bytes.set(bytes, offset);\n						this.values = void 0;\n					}\n				},\n				{\n					name: "save committed latch",\n					address: MEMORY.saveCommitted,\n					bytes: this.committed,\n					writable: false\n				},\n				{\n					name: "save commit command",\n					address: MEMORY.saveControl,\n					length: 1,\n					writable: true,\n					readByte: () => 0,\n					prepareWrite: (_offset, bytes, span, debugEdit) => {\n						if (bytes[0] === 0) return () => void 0;\n						if (bytes[0] !== 1) return void 0;\n						try {\n							this.checkCommitCounter();\n						} catch (error) {\n							throw new RuntimeFault("M019012", error instanceof Error ? error.message : "save counter exhausted", span);\n						}\n						if (debugEdit !== true) charge(HARDWARE.saveCapacityBytes, span);\n						return () => {\n							this.latch();\n							this.writes.clear();\n						};\n					}\n				},\n				{\n					name: "save status",\n					address: MEMORY.saveControl + 1,\n					length: 31,\n					writable: false,\n					readByte: (offset) => {\n						const status = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(32));\n						status.setUint8(1, (this.dirtyBytes > 0 ? 1 : 0) | (this.pendingCommit ? 2 : 0));\n						status.setUint32(4, HARDWARE.saveCapacityBytes, true);\n						status.setBigUint64(8, BigInt(this.commits), true);\n						status.setUint32(16, this.dirtyBytes, true);\n						return status.getUint8(offset + 1);\n					}\n				}\n			];\n		}\n		readValues() {\n			this.values ??= decodeSaveValues(this.bytes);\n			return this.values;\n		}\n		checkCommitCounter() {\n			if (this.commits === Number.MAX_SAFE_INTEGER) throw new RangeError("save commit counter exhausted");\n		}\n		latch() {\n			this.committed.set(this.bytes);\n			this.pendingCommit = true;\n			this.dirtyBytes = 0;\n			this.commits += 1;\n		}\n	};\n	function isSaveImage(value) {\n		return value instanceof Uint8Array ? value.length <= HARDWARE.saveCapacityBytes : isSaveValues(value);\n	}\n	function isSaveSnapshot(value) {\n		return isRecord$3(value) && Object.keys(value).length === 5 && value.revision === 1 && value.bytes instanceof Uint8Array && value.bytes.length === HARDWARE.saveCapacityBytes && value.committed instanceof Uint8Array && value.committed.length === HARDWARE.saveCapacityBytes && typeof value.pendingCommit === "boolean" && typeof value.commits === "number" && Number.isSafeInteger(value.commits) && value.commits >= 0;\n	}\n	function isPendingDeviceWrites(value, snapshot) {\n		if (!Array.isArray(value)) return false;\n		if (value.length === 0) return Object.keys(value).length === 0;\n		if (!snapshot.pendingCommit) return false;\n		try {\n			return isPendingSaveWrites(value, decodeSaveValues(snapshot.committed));\n		} catch {\n			return false;\n		}\n	}\n	function decodeSaveValues(bytes) {\n		if (bytes.length > HARDWARE.saveCapacityBytes) throw new TypeError("save image exceeds the 8 KiB capacity");\n		const zero = bytes.indexOf(0);\n		const end = zero === -1 ? bytes.length : zero;\n		if (end > HARDWARE.saveCapacityBytes || zero !== -1 && bytes.subarray(zero).some((byte) => byte !== 0)) throw new TypeError("save bytes are not an integer-save image");\n		if (end === 0) return {};\n		let values;\n		try {\n			values = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(0, end)));\n		} catch {\n			throw new TypeError("save bytes are not an integer-save image");\n		}\n		if (!isSaveValues(values)) throw new TypeError("save bytes are not an integer-save image");\n		return values;\n	}\n	function isPendingSaveWrites(value, values) {\n		if (!Array.isArray(value) || value.length > Object.keys(values).length || Object.keys(value).length !== value.length) return false;\n		const keys = /* @__PURE__ */ new Set();\n		for (const write of value) {\n			if (!isRecord$3(write) || Object.keys(write).length !== 2 || typeof write.key !== "string" || !Object.hasOwn(values, write.key) || write.value !== values[write.key] || keys.has(write.key)) return false;\n			keys.add(write.key);\n		}\n		return true;\n	}\n	function isSaveValues(value) {\n		if (!isRecord$3(value) || Array.isArray(value)) return false;\n		try {\n			for (const [key, entry] of Object.entries(value)) {\n				validateKey(key);\n				validateInteger(entry);\n			}\n			return encodedBytes(value) <= HARDWARE.saveCapacityBytes;\n		} catch {\n			return false;\n		}\n	}\n	function sortedValues(values) {\n		return Object.fromEntries(Object.entries(values).sort(([left], [right]) => left.localeCompare(right)));\n	}\n	function encodedBytes(values) {\n		return new TextEncoder().encode(JSON.stringify(sortedValues(values))).byteLength;\n	}\n	function encodeValues(values) {\n		if (Object.keys(values).length === 0) return /* @__PURE__ */ new Uint8Array();\n		const bytes = new TextEncoder().encode(JSON.stringify(Object.fromEntries(Object.entries(values).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0))));\n		if (bytes.length > HARDWARE.saveCapacityBytes) throw new RangeError("cartridge save exceeds the 8 KiB capacity");\n		return bytes;\n	}\n	function validateKey(key) {\n		if (!/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(key) || [\n			"__proto__",\n			"constructor",\n			"prototype"\n		].includes(key)) throw new TypeError("save keys must be 1-64 canonical ASCII characters");\n	}\n	function validateInteger(value) {\n		if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new TypeError("save values must be safe integers");\n	}\n	function isRecord$3(value) {\n		return typeof value === "object" && value !== null;\n	}\n	//#endregion\n	//#region src/protocol.ts\n	function isHostRequest(value) {\n		if (!isRecord$2(value) || !isNonNegativeInteger(value.id) || typeof value.type !== "string") return false;\n		switch (value.type) {\n			case "load": return hasExactKeys(value, [\n				"id",\n				"type",\n				"moduleUrl",\n				"configuration"\n			]) && typeof value.moduleUrl === "string" && value.moduleUrl.startsWith("blob:") && isSandboxConfiguration(value.configuration);\n			case "frame": return hasExactKeys(value, [\n				"id",\n				"type",\n				"input"\n			]) && isInputFrame(value.input);\n			case "debug-step": return hasExactKeys(value, [\n				"id",\n				"type",\n				"input"\n			]) && isInputFrame(value.input);\n			case "snapshot":\n			case "audit": return hasExactKeys(value, ["id", "type"]);\n			case "restore": return hasExactKeys(value, [\n				"id",\n				"type",\n				"snapshot"\n			]);\n			case "memory": return hasExactKeys(value, [\n				"id",\n				"type",\n				"address",\n				"length"\n			]) && isMemoryRange(value.address, value.length);\n			case "memory-edit": return hasExactKeys(value, [\n				"id",\n				"type",\n				"address",\n				"bytes"\n			]) && value.bytes instanceof Uint8Array && value.bytes.length > 0 && value.bytes.length <= 256 && isMemoryRange(value.address, value.bytes.length);\n			default: return false;\n		}\n	}\n	function isMemoryRange(address, length) {\n		return typeof address === "number" && Number.isSafeInteger(address) && address >= 0 && typeof length === "number" && Number.isSafeInteger(length) && length > 0 && length <= 256 && address <= 4194304 - length;\n	}\n	function isSandboxConfiguration(value) {\n		return isRecord$2(value) && hasExactKeys(value, [\n			"seed",\n			"workUnitsPerFrame",\n			"updateRate",\n			...value.hardwareRevision === void 0 ? [] : ["hardwareRevision"],\n			...value.maps === void 0 ? [] : ["maps"],\n			...value.save === void 0 ? [] : ["save"],\n			...value.debug === void 0 ? [] : ["debug"],\n			...value.assets === void 0 ? [] : ["assets"],\n			...value.rom === void 0 ? [] : ["rom"]\n		]) && Number.isSafeInteger(value.seed) && isNonNegativeInteger(value.workUnitsPerFrame) && value.workUnitsPerFrame > 0 && value.workUnitsPerFrame <= HARDWARE.workUnitsPerFrame && (value.updateRate === 30 || value.updateRate === 60) && (value.hardwareRevision === void 0 || value.hardwareRevision === 1 || value.hardwareRevision === 2) && (value.maps === void 0 || isMapQueryCatalog(value.maps)) && (value.save === void 0 || isSaveImage(value.save)) && (value.debug === void 0 || typeof value.debug === "boolean") && (value.assets === void 0 || isRuntimeAssetSource(value.assets)) && (value.rom === void 0 || value.rom instanceof Uint8Array && value.rom.length > 0 && value.rom.length <= HARDWARE.cartridgeCapacityBytes);\n	}\n	function isRecord$2(value) {\n		return typeof value === "object" && value !== null;\n	}\n	function isNonNegativeInteger(value) {\n		return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;\n	}\n	function hasExactKeys(value, expected) {\n		const actual = Object.keys(value).sort();\n		return actual.length === expected.length && expected.every((key) => actual.includes(key));\n	}\n	//#endregion\n	//#region src/console-runtime.ts\n	function isConsoleRuntimeSnapshot(value) {\n		return isRecord$1(value) && value.revision === 6 && Object.keys(value).length === 7 && isMachineSnapshot(value.machine) && value.machine.revision === 2 && isSaveSnapshot(value.save) && isGraphicsSnapshot(value.graphics) && isSynthSnapshot(value.audio) && isPendingDeviceWrites(value.pendingSaveWrites, value.save) && isMemorySnapshot(value.memory) && graphicsMatchesMemory(value.graphics, value.memory);\n	}\n	/** One production dispatcher for restricted Workers and deterministic headless hosts. */\n	function createConsoleRuntime(factory, configuration) {\n		if (!isSandboxConfiguration(configuration)) throw new RuntimeFault("M019100", "invalid sandbox configuration", {\n			start: 0,\n			end: 0\n		});\n		const source = configuration.assets;\n		const assets = source === void 0 ? decodeRuntimeAssets({}, {}) : decodeRuntimeAssets(source.declarations, source.files, source.displayPath);\n		const visualStore = new VisualAssetStore(assets.visual, assets.display);\n		const graphics = new IndexedGraphics(visualStore);\n		const audioStore = new AudioAssetStore(assets.audio);\n		const synthesizer = new Synthesizer(audioStore);\n		let rendering = false;\n		let machine = void 0;\n		let drawCommands = [];\n		let audioCommands = [];\n		let frameOutput;\n		let debugFrameActive = false;\n		const mapQueries = new MapQueryStore(source === void 0 ? configuration.maps ?? [] : []);\n		const saveMemory = new SaveMemory(configuration.save ?? {});\n		const cartridgeRom = configuration.rom?.slice() ?? /* @__PURE__ */ new Uint8Array();\n		const cartridgeInfo = createCartridgeInfo(cartridgeRom, configuration.hardwareRevision ?? 1);\n		const debugEnabled = configuration.debug ?? false;\n		let debugTrace = [];\n		const debugCallStack = [];\n		let debugTraceTruncated = false;\n		const ram = new Uint8Array(MEMORY.ramBytes);\n		const bus = new MemoryBus([\n			{\n				name: "ram",\n				address: MEMORY.ram,\n				bytes: ram,\n				writable: true\n			},\n			...graphics.memoryRegions(),\n			...visualStore.memoryRegions(),\n			...synthesizer.memoryRegions(),\n			...saveMemory.memoryRegions((units, span) => {\n				requireMachine().work(units, span);\n			}),\n			{\n				name: "cartridge status",\n				address: MEMORY.cartridgeInfo,\n				bytes: cartridgeInfo,\n				writable: false\n			},\n			...cartridgeRom.length === 0 ? [] : [{\n				name: "canonical cartridge ROM",\n				address: MEMORY.cartridgeRom,\n				bytes: cartridgeRom.subarray(0, 262144),\n				writable: false\n			}, ...cartridgeRom.length > 262144 ? [{\n				name: "canonical cartridge ROM extension",\n				address: MEMORY.cartridgeRomExtension,\n				bytes: cartridgeRom.subarray(262144),\n				writable: false\n			}] : []],\n			{\n				name: "controllers and pointer",\n				address: MEMORY.input,\n				length: MEMORY.inputBytes,\n				writable: false,\n				readByte: (offset) => requireMachine().readInputByte(offset)\n			},\n			{\n				name: "scheduler, RNG, work and fault status",\n				address: MEMORY.system,\n				length: MEMORY.systemBytes,\n				writable: false,\n				readByte: (offset) => requireMachine().readSystemByte(offset)\n			},\n			{\n				name: "master palette RGBA",\n				address: MEMORY.palette,\n				bytes: Uint8Array.from(MASTER_PALETTE_RGBA.slice(0, 128)),\n				writable: false\n			},\n			{\n				name: "extended master palette RGBA",\n				address: MEMORY.paletteExtended,\n				bytes: Uint8Array.from(MASTER_PALETTE_RGBA.slice(128)),\n				writable: false\n			}\n		], (units, span) => {\n			requireMachine().work(units, span);\n		});\n		machine = new DeterministicMachine(factory, configuration, {\n			completeFrame: () => {\n				frameOutput = {\n					indexedPixels: graphics.finishFrame().indexedPixels,\n					audio: synthesizer.finishFrame(),\n					audioState: synthesizer.snapshot()\n				};\n			},\n			call: handleConsoleCall,\n			...debugEnabled ? {\n				probe: (id, sourceSpan, locals) => {\n					if (debugTrace.length >= HARDWARE.drawCommandsPerFrame) {\n						debugTraceTruncated = true;\n						return;\n					}\n					debugTrace.push({\n						id,\n						sourceSpan,\n						locals: structuredClone(locals),\n						callStack: structuredClone(debugCallStack)\n					});\n				},\n				enter: (name, sourceSpan) => {\n					debugCallStack.push({\n						name,\n						sourceSpan\n					});\n				},\n				leave: () => {\n					debugCallStack.pop();\n				}\n			} : {}\n		});\n		graphics.beginFrame();\n		rendering = true;\n		try {\n			if (!debugEnabled) machine.boot();\n		} finally {\n			rendering = false;\n		}\n		graphics.finishFrame();\n		let boundarySnapshot = captureSnapshot();\n		return {\n			runFrame(input) {\n				if (!isInputFrame(input)) throw new RuntimeFault("M019008", "invalid controller input frame", {\n					start: 0,\n					end: 0\n				});\n				const active = requireMachine();\n				active.assertRunnable();\n				drawCommands = [];\n				audioCommands = [];\n				frameOutput = void 0;\n				debugTrace = [];\n				debugTraceTruncated = false;\n				graphics.beginFrame();\n				rendering = true;\n				let report;\n				try {\n					report = active.runFrame(input);\n				} finally {\n					rendering = false;\n				}\n				const output = completedOutput();\n				const saveWrites = saveMemory.takeWrites();\n				const saveCommit = saveMemory.takeCommit();\n				boundarySnapshot = captureSnapshot();\n				return {\n					...report,\n					drawCommands,\n					audioCommands,\n					saveWrites,\n					...saveCommit === void 0 ? {} : { saveCommit },\n					output,\n					...debugEnabled ? { debug: {\n						trace: debugTrace,\n						truncated: debugTraceTruncated,\n						inspection: structuredClone(active.inspect())\n					} } : {}\n				};\n			},\n			stepDebug(input) {\n				if (!debugEnabled) throw new RuntimeFault("M019104", "statement stepping requires a debug cartridge", {\n					start: 0,\n					end: 0\n				});\n				if (!isInputFrame(input)) throw new RuntimeFault("M019008", "invalid controller input frame", {\n					start: 0,\n					end: 0\n				});\n				if (!debugFrameActive) {\n					drawCommands = [];\n					audioCommands = [];\n					frameOutput = void 0;\n					debugTrace = [];\n					debugTraceTruncated = false;\n					graphics.beginFrame();\n					debugFrameActive = true;\n				}\n				rendering = true;\n				let step;\n				try {\n					step = requireMachine().stepDebug(input);\n				} finally {\n					rendering = false;\n				}\n				if (step.event !== void 0) {\n					const inspection = structuredClone(requireMachine().inspect());\n					return {\n						event: {\n							...step.event,\n							callStack: structuredClone(debugCallStack)\n						},\n						inspection\n					};\n				}\n				if (step.booted === true) {\n					debugFrameActive = false;\n					graphics.finishFrame();\n					boundarySnapshot = captureSnapshot();\n					return { booted: true };\n				}\n				const report = step.report;\n				if (report === void 0) throw new TypeError("debug step produced no event or frame");\n				debugFrameActive = false;\n				const output = completedOutput();\n				const saveWrites = saveMemory.takeWrites();\n				const saveCommit = saveMemory.takeCommit();\n				const inspection = structuredClone(requireMachine().inspect());\n				boundarySnapshot = captureSnapshot();\n				return { frame: {\n					...report,\n					drawCommands,\n					audioCommands,\n					saveWrites,\n					...saveCommit === void 0 ? {} : { saveCommit },\n					output,\n					debug: {\n						trace: debugTrace,\n						truncated: debugTraceTruncated,\n						inspection\n					}\n				} };\n			},\n			snapshot: captureSnapshot,\n			restore(value) {\n				const snapshot = readWorkerSnapshot(value);\n				const before = debugFrameActive ? boundarySnapshot : captureSnapshot();\n				try {\n					requireMachine().restore(snapshot.machine);\n					debugFrameActive = false;\n					if (snapshot.revision === 6) saveMemory.restoreDevice(snapshot.save, snapshot.pendingSaveWrites);\n					else saveMemory.restore(snapshot.save, snapshot.pendingSaveWrites);\n					if (snapshot.revision >= 2) {\n						graphics.restore(snapshot.graphics);\n						synthesizer.restore(snapshot.audio);\n					}\n					if (snapshot.revision >= 4) bus.restore(snapshot.memory);\n					else if (snapshot.revision >= 2) {\n						const visual = new VisualAssetStore(assets.visual, assets.display).memoryRegions()[0];\n						if (visual === void 0) throw new TypeError("missing visual image");\n						if (snapshot.revision === 3 && isMemorySnapshot(snapshot.memory)) bus.restore({\n							revision: 1,\n							regions: [...snapshot.memory.regions, {\n								address: MEMORY.visual,\n								bytes: visual.bytes\n							}].sort((a, b) => a.address - b.address)\n						});\n						else {\n							ram.fill(0);\n							visualStore.memoryRegions()[0]?.bytes.set(visual.bytes);\n						}\n					}\n					boundarySnapshot = captureSnapshot();\n				} catch (error) {\n					requireMachine().restore(before.machine);\n					saveMemory.restoreDevice(before.save, before.pendingSaveWrites);\n					graphics.restore(before.graphics);\n					synthesizer.restore(before.audio);\n					bus.restore(before.memory);\n					throw new RuntimeFault("M019103", error instanceof Error ? error.message : "invalid device snapshot", {\n						start: 0,\n						end: 0\n					});\n				}\n			},\n			inspectMemory(address, length) {\n				if (!debugEnabled) throw new RuntimeFault("M019104", "memory inspection requires a debug cartridge", {\n					start: 0,\n					end: 0\n				});\n				if (!Number.isSafeInteger(length) || length < 1 || length > 256) throw new RuntimeFault("M019020", "debug memory reads require 1-256 bytes", {\n					start: 0,\n					end: 0\n				});\n				return {\n					address,\n					bytes: bus.inspect(address, length),\n					regions: bus.describe()\n				};\n			},\n			editMemory(address, bytes) {\n				if (!debugEnabled) throw new RuntimeFault("M019104", "memory editing requires a debug cartridge", {\n					start: 0,\n					end: 0\n				});\n				if (!(bytes instanceof Uint8Array) || bytes.length < 1 || bytes.length > 256) throw new RuntimeFault("M019020", "debug memory edits require 1-256 bytes", {\n					start: 0,\n					end: 0\n				});\n				bus.edit(address, bytes);\n			},\n			memoryRegions() {\n				return bus.describe();\n			}\n		};\n		function captureSnapshot() {\n			return {\n				revision: 6,\n				machine: requireMachine().snapshot(),\n				save: saveMemory.deviceSnapshot(),\n				graphics: graphics.snapshot(),\n				audio: synthesizer.snapshot(),\n				pendingSaveWrites: saveMemory.pendingWrites(),\n				memory: bus.snapshot()\n			};\n		}\n		function completedOutput() {\n			if (frameOutput === void 0) throw new RuntimeFault("M019102", "frame completed without device output", {\n				start: 0,\n				end: 0\n			});\n			return frameOutput;\n		}\n		function handleConsoleCall(name, arguments_, sourceSpan, context) {\n			if (name === "visual_id" || name === "audio_id") {\n				if (arguments_.length !== 1 || typeof arguments_[0] !== "string") throw new RuntimeFault("M019009", `${name} expects one Text name`, sourceSpan);\n				requireMachine().work(1 + arguments_[0].length, sourceSpan);\n				return (name === "visual_id" ? visualStore : audioStore).id(arguments_[0]);\n			}\n			if (MEMORY_CALLS.has(name)) {\n				if (arguments_.length !== MEMORY_CALLS.get(name)) throw new RuntimeFault("M019009", `${name} received the wrong argument count`, sourceSpan);\n				const values = arguments_.map((value) => readInteger(value, sourceSpan));\n				const address = values[0] ?? 0;\n				const second = values[1] ?? 0;\n				const third = values[2] ?? 0;\n				const raster = context.phase === "raster";\n				switch (name) {\n					case "mem_read": return bus.read(address, 1, sourceSpan);\n					case "mem_read16": return bus.read(address, 2, sourceSpan);\n					case "mem_write":\n						bus.write(address, second, 1, sourceSpan, raster);\n						return;\n					case "mem_write16":\n						bus.write(address, second, 2, sourceSpan, raster);\n						return;\n					case "mem_copy":\n						bus.copy(address, second, third, sourceSpan, raster);\n						return;\n					case "mem_fill":\n						bus.fill(address, second, third, sourceSpan, raster);\n						return;\n				}\n			}\n			requireMachine().work(consoleWorkCost(name, arguments_), sourceSpan);\n			if (context.phase === "raster" && name !== "pal" && name !== "raster_scroll") throw new RuntimeFault("M019011", `console API call \'${name}\' is not valid in the raster callback`, sourceSpan);\n			if (name === "raster_scroll" && context.phase !== "raster") throw new RuntimeFault("M019011", "raster_scroll is only valid in the raster callback", sourceSpan);\n			if (name === "save_commit") {\n				if (arguments_.length !== 0) throw new RuntimeFault("M019009", "save_commit expects no arguments", sourceSpan);\n				requireMachine().work(HARDWARE.saveCapacityBytes, sourceSpan);\n				try {\n					saveMemory.commit();\n				} catch (error) {\n					throw new RuntimeFault("M019012", error instanceof Error ? error.message : "invalid save commit", sourceSpan);\n				}\n				return;\n			}\n			if (name === "map_cell" || name === "map_flag") {\n				const handle = arguments_[0];\n				if (!isAssetHandle(handle, "Map")) throw new RuntimeFault("M019009", "expected a Map asset handle", sourceSpan);\n				const integers = arguments_.slice(1).map((value) => readInteger(value, sourceSpan));\n				if (name === "map_cell" && integers.length === 3) {\n					if (source !== void 0) return visualStore.mapCell(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0) ?? -1;\n					return mapQueries.cell(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0);\n				}\n				if (name === "map_flag" && integers.length === 4) {\n					if (source !== void 0) return visualStore.mapFlag(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0, integers[3] ?? 0);\n					return mapQueries.flag(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0, integers[3] ?? 0);\n				}\n				throw new RuntimeFault("M019009", `${name} received the wrong argument count`, sourceSpan);\n			}\n			if (name === "save_get_int" || name === "save_set_int") {\n				const key = arguments_[0];\n				if (typeof key !== "string") throw new RuntimeFault("M019009", "save key must be Text", sourceSpan);\n				try {\n					if (name === "save_get_int" && arguments_.length === 2) return saveMemory.get(key, readInteger(arguments_[1], sourceSpan));\n					if (name === "save_set_int" && arguments_.length === 2) {\n						saveMemory.set(key, readInteger(arguments_[1], sourceSpan));\n						return;\n					}\n				} catch (error) {\n					throw new RuntimeFault("M019012", error instanceof Error ? error.message : "invalid cartridge save operation", sourceSpan);\n				}\n				throw new RuntimeFault("M019009", `${name} received the wrong argument count`, sourceSpan);\n			}\n			const command = {\n				name,\n				arguments: structuredClone(arguments_),\n				sourceSpan,\n				...context.rasterLine === void 0 ? {} : { rasterLine: context.rasterLine }\n			};\n			if (DRAW_CALLS.has(name)) {\n				if (drawCommands.length >= HARDWARE.drawCommandsPerFrame) throw new RuntimeFault("M019010", "draw-command ceiling exceeded", sourceSpan);\n				drawCommands.push(command);\n				if (rendering) graphics.executeCommand(command);\n				return;\n			}\n			if (AUDIO_CALLS.has(name)) {\n				audioCommands.push(command);\n				if (rendering) synthesizer.executeCommand(command);\n				return;\n			}\n			throw new RuntimeFault("M019004", `console API call \'${name}\' is unavailable`, sourceSpan);\n		}\n		function requireMachine() {\n			if (machine === void 0) throw new RuntimeFault("M019102", "no cartridge is loaded", {\n				start: 0,\n				end: 0\n			});\n			return machine;\n		}\n	}\n	function createCartridgeInfo(rom, hardwareRevision) {\n		const bytes = /* @__PURE__ */ new Uint8Array(64);\n		const view = new DataView(bytes.buffer);\n		const validHeader = rom.length >= 12 && [\n			77,\n			79,\n			68,\n			45,\n			48,\n			49,\n			26\n		].every((byte, index) => rom[index] === byte) && rom[7] === 1;\n		view.setUint16(0, hardwareRevision, true);\n		view.setUint16(2, validHeader ? rom[7] ?? 0 : 0, true);\n		view.setUint32(4, rom.length, true);\n		view.setUint32(8, hardwareRevision === 1 ? 262144 : HARDWARE.cartridgeCapacityBytes, true);\n		view.setUint32(12, MEMORY.cartridgeRom, true);\n		view.setUint32(16, Number(rom.length > 0) | Number(validHeader) << 1, true);\n		view.setUint32(20, validHeader ? new DataView(rom.buffer, rom.byteOffset).getUint32(8, true) : 0, true);\n		return bytes;\n	}\n	const DRAW_CALLS = /* @__PURE__ */ new Set([\n		"clear",\n		"pixel",\n		"line",\n		"rect",\n		"rect_fill",\n		"circle",\n		"circle_fill",\n		"triangle",\n		"camera",\n		"clip",\n		"clip_reset",\n		"sprite",\n		"sprite_xform",\n		"animation",\n		"map",\n		"pal",\n		"pal_reset",\n		"raster_scroll",\n		"print",\n		"font_print"\n	]);\n	const AUDIO_CALLS = /* @__PURE__ */ new Set([\n		"sfx",\n		"music",\n		"music_stop"\n	]);\n	function consoleWorkCost(name, arguments_) {\n		const integer = (index) => {\n			const value = arguments_[index];\n			return typeof value === "number" && Number.isSafeInteger(value) ? value : 0;\n		};\n		switch (name) {\n			case "clear": return Math.ceil(HARDWARE.width * HARDWARE.height / 32);\n			case "pixel": return 1;\n			case "line": return Math.max(Math.abs(integer(2) - integer(0)), Math.abs(integer(3) - integer(1))) + 1;\n			case "rect": return Math.max(1, 2 * Math.abs(integer(2)) + 2 * Math.abs(integer(3)));\n			case "rect_fill": return Math.max(1, Math.ceil(Math.abs(integer(2)) * Math.abs(integer(3)) / 4));\n			case "circle": return Math.max(1, Math.abs(integer(2)) * 8);\n			case "circle_fill": return Math.max(1, Math.ceil(Math.abs(integer(2)) ** 2 * 3 / 4));\n			case "triangle": {\n				const area = Math.abs((integer(2) - integer(0)) * (integer(5) - integer(1)) - (integer(4) - integer(0)) * (integer(3) - integer(1)));\n				return Math.max(1, Math.ceil(area / 8));\n			}\n			case "sprite":\n			case "animation": return 32;\n			case "sprite_xform": return Math.max(32, 32 * Math.abs(integer(3)) ** 2);\n			case "map": return 128;\n			case "print": return Math.max(1, (typeof arguments_[0] === "string" ? arguments_[0].length : 0) * 6);\n			case "font_print": return Math.max(1, (typeof arguments_[1] === "string" ? arguments_[1].length : 0) * 8);\n			case "sfx":\n			case "music":\n			case "music_stop": return 8;\n			default: return 1;\n		}\n	}\n	function isRecord$1(value) {\n		return typeof value === "object" && value !== null;\n	}\n	function isAssetHandle(value, kind) {\n		return isRecord$1(value) && typeof value.name === "string" && value.name.length > 0 && value.kind === kind;\n	}\n	function readInteger(value, sourceSpan) {\n		if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new RuntimeFault("M019009", "console arguments must be safe integers", sourceSpan);\n		return value;\n	}\n	function readWorkerSnapshot(value) {\n		if (!isRecord$1(value) || (value.revision === 6 ? !isConsoleRuntimeSnapshot(value) : !isLegacyWorkerSnapshot(value))) throw new RuntimeFault("M019103", "invalid worker snapshot", {\n			start: 0,\n			end: 0\n		});\n		return {\n			revision: value.revision,\n			machine: value.machine,\n			save: value.save,\n			graphics: value.graphics,\n			audio: value.audio,\n			pendingSaveWrites: value.revision === 1 ? [] : value.pendingSaveWrites,\n			memory: value.memory\n		};\n	}\n	function isLegacyWorkerSnapshot(value) {\n		if (!isMachineSnapshot(value.machine) || value.machine.revision !== (value.revision === 5 ? 2 : 1) || !isSaveValues(value.save)) return false;\n		if (value.revision === 1) return Object.keys(value).length === 3;\n		if (value.revision !== 2 && value.revision !== 3 && value.revision !== 4 && value.revision !== 5) return false;\n		return Object.keys(value).length === (value.revision === 2 ? 6 : 7) && isGraphicsSnapshot(value.graphics) && isSynthSnapshot(value.audio) && isPendingSaveWrites(value.pendingSaveWrites, value.save) && (value.revision === 2 || isMemorySnapshot(value.memory) && graphicsMatchesMemory(value.graphics, value.memory));\n	}\n	function graphicsMatchesMemory(graphics, memory) {\n		return [[MEMORY.front, graphics.front], [MEMORY.display, graphics.resolved]].every(([address, pixels]) => {\n			if (!(pixels instanceof Uint8Array)) return false;\n			const bytes = memory.regions.find((region) => region.address === address)?.bytes;\n			return bytes !== void 0 && bytes.length === pixels.length && bytes.every((value, index) => value === pixels[index]);\n		});\n	}\n	const MEMORY_CALLS = /* @__PURE__ */ new Map([\n		["mem_read", 1],\n		["mem_read16", 1],\n		["mem_write", 2],\n		["mem_write16", 2],\n		["mem_copy", 3],\n		["mem_fill", 3]\n	]);\n	//#endregion\n	//#region src/sandbox-worker.ts\n	const workerPort = globalThis;\n	const send = workerPort.postMessage.bind(workerPort);\n	let runtime;\n	lockDownWorkerGlobals(globalThis);\n	workerPort.onmessage = (event) => {\n		if (!isHostRequest(event.data)) {\n			send({\n				id: requestId(event.data),\n				type: "error",\n				code: "M019100",\n				message: "invalid sandbox protocol message"\n			});\n			return;\n		}\n		const request = event.data;\n		handleRequest(request).catch((error) => {\n			const response = errorResponse(request.id, error);\n			send(response);\n		});\n	};\n	async function handleRequest(request) {\n		switch (request.type) {\n			case "load":\n				runtime = createConsoleRuntime(readFactory(await import(\n					/* @vite-ignore */\n					request.moduleUrl\n)), request.configuration);\n				send({\n					id: request.id,\n					type: "loaded"\n				});\n				break;\n			case "frame":\n				send({\n					id: request.id,\n					type: "frame",\n					...requireRuntime().runFrame(request.input)\n				});\n				break;\n			case "debug-step": {\n				const result = requireRuntime().stepDebug(request.input);\n				send({\n					id: request.id,\n					type: "debug-step",\n					..."frame" in result ? { frame: result.frame } : "booted" in result ? { booted: true } : {\n						event: result.event,\n						inspection: result.inspection\n					}\n				});\n				break;\n			}\n			case "audit": {\n				const globals = globalThis;\n				const math = globals.Math;\n				send({\n					id: request.id,\n					type: "audit",\n					exposedCapabilities: DENIED_WORKER_CAPABILITIES.filter((capability) => globals[capability] !== void 0),\n					mathRandomAvailable: typeof math === "object" && math !== null && typeof math.random === "function"\n				});\n				break;\n			}\n			case "snapshot":\n				send({\n					id: request.id,\n					type: "snapshot",\n					snapshot: requireRuntime().snapshot()\n				});\n				break;\n			case "restore":\n				requireRuntime().restore(request.snapshot);\n				send({\n					id: request.id,\n					type: "restored"\n				});\n				break;\n			case "memory":\n				send({\n					id: request.id,\n					type: "memory",\n					...requireRuntime().inspectMemory(request.address, request.length)\n				});\n				break;\n			case "memory-edit":\n				requireRuntime().editMemory(request.address, request.bytes);\n				send({\n					id: request.id,\n					type: "memory-edited"\n				});\n		}\n	}\n	function readFactory(module) {\n		if (!isRecord(module) || typeof module.default !== "function") throw new RuntimeFault("M019101", "compiled module does not export a cartridge factory", {\n			start: 0,\n			end: 0\n		});\n		return module.default;\n	}\n	function requireRuntime() {\n		if (runtime === void 0) throw new RuntimeFault("M019102", "no cartridge is loaded", {\n			start: 0,\n			end: 0\n		});\n		return runtime;\n	}\n	function errorResponse(id, error) {\n		if (error instanceof RuntimeFault) return {\n			id,\n			type: "error",\n			code: error.code,\n			message: error.message,\n			sourceSpan: error.sourceSpan\n		};\n		return {\n			id,\n			type: "error",\n			code: "M019199",\n			message: error instanceof Error ? error.message : "unknown sandbox failure"\n		};\n	}\n	function requestId(value) {\n		return isRecord(value) && typeof value.id === "number" && Number.isSafeInteger(value.id) ? value.id : 0;\n	}\n	function isRecord(value) {\n		return typeof value === "object" && value !== null;\n	}\n	//#endregion\n})();\n';
  var blob =
    typeof self !== 'undefined' &&
    self.Blob &&
    new Blob(['(self.URL || self.webkitURL).revokeObjectURL(self.location.href);', jsContent], {
      type: 'text/javascript;charset=utf-8',
    });
  function WorkerWrapper(options) {
    let objURL;
    try {
      objURL = blob && (self.URL || self.webkitURL).createObjectURL(blob);
      if (!objURL) throw '';
      const worker = new Worker(objURL, { name: options?.name });
      worker.addEventListener('error', () => {
        (self.URL || self.webkitURL).revokeObjectURL(objURL);
      });
      return worker;
    } catch (e) {
      return new Worker('data:text/javascript;charset=utf-8,' + encodeURIComponent(jsContent), {
        name: options?.name,
      });
    }
  }
  //#endregion
  //#region src/hardware.ts
  var HARDWARE = Object.freeze({
    width: 240,
    height: 144,
    frameRate: 60,
    paletteSize: 256,
    rasterPaletteSize: 32,
    transparentColor: 0,
    visualCapacityBytes: 1048576,
    saveCapacityBytes: 8192,
    cartridgeCapacityBytes: 1048576,
    drawCommandsPerFrame: 4096,
    workUnitsPerFrame: 5e4,
    audioVoices: 8,
    audioSampleRate: 48e3,
    trackerChannels: 8,
    controllerPorts: 4,
    spriteMaximumAxis: 128,
    tileSize: 8,
  });
  /** Hardware Revision 2 keeps the original 32 colours, then adds a 216-colour cube and eight greys. */
  var MASTER_PALETTE = Object.freeze([
    ...[
      '#17141f',
      '#292532',
      '#403946',
      '#5d5054',
      '#806a63',
      '#aa8b74',
      '#d5b992',
      '#f4e5bd',
      '#5b2938',
      '#8b3c47',
      '#bf5558',
      '#ed7b69',
      '#5a3928',
      '#89572e',
      '#c18436',
      '#e7bd50',
      '#263c32',
      '#345f46',
      '#4b8b58',
      '#7fbd68',
      '#203b47',
      '#2e6571',
      '#43969a',
      '#75cbc0',
      '#243451',
      '#345581',
      '#4b7db3',
      '#73a9d1',
      '#3e3154',
      '#654777',
      '#936397',
      '#c38aae',
    ],
    ...Array.from({ length: 216 }, (_, index) => {
      return `#${[Math.floor(index / 36) * 51, (Math.floor(index / 6) % 6) * 51, (index % 6) * 51]
        .map((value) => value.toString(16).padStart(2, '0'))
        .join('')}`;
    }),
    ...Array.from({ length: 8 }, (_, index) => {
      const value = (index * 29 + 26).toString(16).padStart(2, '0');
      return `#${value}${value}${value}`;
    }),
  ]);
  var MASTER_PALETTE_RGBA = Object.freeze(
    MASTER_PALETTE.flatMap((hex) => [
      Number.parseInt(hex.slice(1, 3), 16),
      Number.parseInt(hex.slice(3, 5), 16),
      Number.parseInt(hex.slice(5, 7), 16),
      255,
    ]),
  );
  //#endregion
  //#region src/errors.ts
  var RuntimeFault = class extends Error {
    code;
    sourceSpan;
    constructor(code, message, sourceSpan) {
      super(message);
      this.name = 'RuntimeFault';
      this.code = code;
      this.sourceSpan = sourceSpan;
    }
  };
  Object.freeze({
    size: 4194304,
    ram: 0,
    ramBytes: 65536,
    front: 65536,
    back: 102400,
    display: 139264,
    visual: 196608,
    visualExtended: 2162688,
    draw: 327680,
    transparency: 327760,
    palette: 327808,
    input: 327936,
    inputBytes: 48,
    system: 328192,
    systemBytes: 64,
    rasterLive: 328704,
    visualInfo: 328448,
    saveControl: 328960,
    cartridgeInfo: 329216,
    save: 360448,
    saveCommitted: 368640,
    cartridgeRom: 393216,
    cartridgeRomExtension: 1048576,
    paletteExtended: 2097152,
    audio: 331776,
    audioTracker: 331808,
    audioVoices: 332032,
    voiceStride: 64,
    audioAssets: 3932160,
    audioAssetStride: 32,
    raster: 348160,
    rasterStride: 40,
    assets: 655360,
    assetStride: 32,
    allocations: 786432,
    allocationStride: 24,
  });
  //#endregion
  //#region src/audio.ts
  function isSynthSnapshot(value) {
    if (
      !audioRecord(value) ||
      Object.keys(value).length !== 5 ||
      value.revision !== 1 ||
      !audioCounter(value.frame) ||
      !audioCounter(value.nextSequence) ||
      value.nextSequence < 1 ||
      !denseAudioArray(value.voices) ||
      value.voices.length !== HARDWARE.audioVoices
    )
      return false;
    const nextSequence = value.nextSequence;
    if (!value.voices.every((voice, slot) => isVoice(voice, slot, nextSequence))) return false;
    return (
      value.tracker === null ||
      (audioRecord(value.tracker) &&
        Object.keys(value.tracker).length === 4 &&
        typeof value.tracker.music === 'string' &&
        value.tracker.music.length > 0 &&
        value.tracker.music.length <= HARDWARE.cartridgeCapacityBytes &&
        audioCounter(value.tracker.orderIndex) &&
        audioCounter(value.tracker.row) &&
        audioCounter(value.tracker.frameInRow))
    );
  }
  function isVoice(voice, slot, nextSequence) {
    return (
      audioRecord(voice) &&
      Object.keys(voice).length === 9 &&
      typeof voice.active === 'boolean' &&
      voice.slot === slot &&
      typeof voice.sound === 'string' &&
      voice.sound.length <= HARDWARE.cartridgeCapacityBytes &&
      audioNumber(voice.note, 0, 127) &&
      audioNumber(voice.volumeScale, 0, 1) &&
      audioCounter(voice.ageFrames) &&
      audioNumber(voice.phase, 0, 1) &&
      voice.phase < 1 &&
      audioCounter(voice.noiseState) &&
      voice.noiseState > 0 &&
      voice.noiseState <= 4294967295 &&
      audioCounter(voice.sequence) &&
      voice.sequence < nextSequence
    );
  }
  function isAudioFrame(value) {
    const samples = HARDWARE.audioSampleRate / HARDWARE.frameRate;
    return (
      audioRecord(value) &&
      Object.keys(value).length === 4 &&
      value.left instanceof Float32Array &&
      value.left.length === samples &&
      value.right instanceof Float32Array &&
      value.right.length === samples &&
      value.left.every((sample) => audioNumber(sample, -1, 1)) &&
      value.right.every((sample) => audioNumber(sample, -1, 1)) &&
      audioCounter(value.activeVoices) &&
      value.activeVoices <= HARDWARE.audioVoices &&
      (value.tracker === null ||
        (audioRecord(value.tracker) &&
          Object.keys(value.tracker).length === 4 &&
          typeof value.tracker.music === 'string' &&
          value.tracker.music.length > 0 &&
          value.tracker.music.length <= HARDWARE.cartridgeCapacityBytes &&
          audioCounter(value.tracker.orderIndex) &&
          audioCounter(value.tracker.row) &&
          audioCounter(value.tracker.frameInRow)))
    );
  }
  function audioCounter(value) {
    return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
  }
  function audioNumber(value, minimum, maximum) {
    return (
      typeof value === 'number' && Number.isFinite(value) && value >= minimum && value <= maximum
    );
  }
  function audioRecord(value) {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
  }
  /** Small Web Audio queue used only after an explicit host-side resume gesture. */
  var WebAudioSink = class {
    context;
    output;
    cursor = 0;
    constructor(context = new AudioContext({ sampleRate: HARDWARE.audioSampleRate }), volume = 1) {
      if (!Number.isFinite(volume) || volume < 0 || volume > 1)
        throw new RangeError('audio volume must be between zero and one');
      this.context = context;
      const gain = context.createGain();
      gain.gain.value = volume;
      gain.connect(context.destination);
      this.output = gain;
    }
    get state() {
      return this.context.state;
    }
    async resume() {
      await this.context.resume();
      this.cursor = Math.max(this.cursor, this.context.currentTime);
    }
    enqueue(frame) {
      if (this.context.state !== 'running') return;
      const buffer = this.context.createBuffer(2, frame.left.length, HARDWARE.audioSampleRate);
      buffer.getChannelData(0).set(frame.left);
      buffer.getChannelData(1).set(frame.right);
      const source = this.context.createBufferSource();
      source.buffer = buffer;
      source.connect(this.output);
      this.cursor = Math.max(this.cursor, this.context.currentTime);
      source.start(this.cursor);
      this.cursor += frame.left.length / HARDWARE.audioSampleRate;
    }
    async close() {
      await this.context.close();
    }
  };
  function denseAudioArray(value) {
    return (
      Array.isArray(value) &&
      Object.keys(value).length === value.length &&
      Array.from(value.keys()).every((index) => Object.hasOwn(value, index))
    );
  }
  Object.freeze({
    glyphWidth: 5,
    glyphHeight: 7,
    advanceX: 6,
    advanceY: 8,
  });
  Object.freeze({
    ' ': [0, 0, 0, 0, 0, 0, 0],
    '!': [4, 4, 4, 4, 4, 0, 4],
    '"': [10, 10, 10, 0, 0, 0, 0],
    '#': [10, 31, 10, 10, 31, 10, 0],
    $: [4, 15, 20, 14, 5, 30, 4],
    '%': [25, 26, 4, 4, 11, 19, 0],
    '&': [12, 18, 20, 8, 21, 18, 13],
    "'": [4, 4, 8, 0, 0, 0, 0],
    '(': [2, 4, 8, 8, 8, 4, 2],
    ')': [8, 4, 2, 2, 2, 4, 8],
    '*': [0, 21, 14, 31, 14, 21, 0],
    '+': [0, 4, 4, 31, 4, 4, 0],
    ',': [0, 0, 0, 0, 4, 4, 8],
    '-': [0, 0, 0, 31, 0, 0, 0],
    '.': [0, 0, 0, 0, 0, 12, 12],
    '/': [1, 2, 2, 4, 8, 8, 16],
    0: [14, 17, 19, 21, 25, 17, 14],
    1: [4, 12, 4, 4, 4, 4, 14],
    2: [14, 17, 1, 2, 4, 8, 31],
    3: [30, 1, 1, 14, 1, 1, 30],
    4: [2, 6, 10, 18, 31, 2, 2],
    5: [31, 16, 16, 30, 1, 1, 30],
    6: [14, 16, 16, 30, 17, 17, 14],
    7: [31, 1, 2, 4, 8, 8, 8],
    8: [14, 17, 17, 14, 17, 17, 14],
    9: [14, 17, 17, 15, 1, 1, 14],
    ':': [0, 12, 12, 0, 12, 12, 0],
    ';': [0, 12, 12, 0, 4, 4, 8],
    '<': [2, 4, 8, 16, 8, 4, 2],
    '=': [0, 0, 31, 0, 31, 0, 0],
    '>': [8, 4, 2, 1, 2, 4, 8],
    '?': [14, 17, 1, 2, 4, 0, 4],
    '@': [14, 17, 23, 21, 23, 16, 14],
    A: [14, 17, 17, 31, 17, 17, 17],
    B: [30, 17, 17, 30, 17, 17, 30],
    C: [14, 17, 16, 16, 16, 17, 14],
    D: [28, 18, 17, 17, 17, 18, 28],
    E: [31, 16, 16, 30, 16, 16, 31],
    F: [31, 16, 16, 30, 16, 16, 16],
    G: [14, 17, 16, 23, 17, 17, 15],
    H: [17, 17, 17, 31, 17, 17, 17],
    I: [14, 4, 4, 4, 4, 4, 14],
    J: [7, 2, 2, 2, 2, 18, 12],
    K: [17, 18, 20, 24, 20, 18, 17],
    L: [16, 16, 16, 16, 16, 16, 31],
    M: [17, 27, 21, 21, 17, 17, 17],
    N: [17, 25, 25, 21, 19, 19, 17],
    O: [14, 17, 17, 17, 17, 17, 14],
    P: [30, 17, 17, 30, 16, 16, 16],
    Q: [14, 17, 17, 17, 21, 18, 13],
    R: [30, 17, 17, 30, 20, 18, 17],
    S: [15, 16, 16, 14, 1, 1, 30],
    T: [31, 4, 4, 4, 4, 4, 4],
    U: [17, 17, 17, 17, 17, 17, 14],
    V: [17, 17, 17, 17, 17, 10, 4],
    W: [17, 17, 17, 21, 21, 21, 10],
    X: [17, 17, 10, 4, 10, 17, 17],
    Y: [17, 17, 10, 4, 4, 4, 4],
    Z: [31, 1, 2, 4, 8, 16, 31],
    '[': [14, 8, 8, 8, 8, 8, 14],
    '\\': [16, 8, 8, 4, 2, 2, 1],
    ']': [14, 2, 2, 2, 2, 2, 14],
    '^': [4, 10, 17, 0, 0, 0, 0],
    _: [0, 0, 0, 0, 0, 0, 31],
    '`': [8, 4, 2, 0, 0, 0, 0],
    '{': [3, 4, 4, 24, 4, 4, 3],
    '|': [4, 4, 4, 4, 4, 4, 4],
    '}': [24, 4, 4, 3, 4, 4, 24],
    '~': [0, 0, 9, 22, 0, 0, 0],
  })['?'];
  //#endregion
  //#region src/graphics.ts
  /** WebGL2 palette resolver. The GPU sees only indexed pixels and the immutable RGB table. */
  var WebGlIndexedRenderer = class {
    gl;
    program;
    indexTexture;
    paletteTexture;
    vertexArray;
    constructor(canvas) {
      canvas.width = HARDWARE.width;
      canvas.height = HARDWARE.height;
      const gl = canvas.getContext('webgl2', {
        alpha: false,
        antialias: false,
        depth: false,
      });
      if (gl === null) throw new Error('WebGL2 is required by the MOD-01 alpha renderer');
      this.gl = gl;
      this.program = createProgram(gl);
      this.indexTexture = requireObject(gl.createTexture(), 'index texture');
      this.paletteTexture = requireObject(gl.createTexture(), 'palette texture');
      this.vertexArray = requireObject(gl.createVertexArray(), 'vertex array');
      gl.bindVertexArray(this.vertexArray);
      gl.useProgram(this.program);
      configureIndexTexture(gl, this.indexTexture);
      configurePaletteTexture(gl, this.paletteTexture);
      setSampler(gl, this.program, 'indexedFrame', 0);
      setSampler(gl, this.program, 'masterPalette', 1);
      gl.viewport(0, 0, HARDWARE.width, HARDWARE.height);
    }
    render(indexedPixels) {
      if (indexedPixels.length !== HARDWARE.width * HARDWARE.height)
        throw new RangeError('indexed frame has the wrong dimensions');
      const gl = this.gl;
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.indexTexture);
      gl.texSubImage2D(
        gl.TEXTURE_2D,
        0,
        0,
        0,
        HARDWARE.width,
        HARDWARE.height,
        gl.RED_INTEGER,
        gl.UNSIGNED_BYTE,
        indexedPixels,
      );
      gl.bindVertexArray(this.vertexArray);
      gl.useProgram(this.program);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }
    destroy() {
      this.gl.deleteTexture(this.indexTexture);
      this.gl.deleteTexture(this.paletteTexture);
      this.gl.deleteVertexArray(this.vertexArray);
      this.gl.deleteProgram(this.program);
    }
  };
  function createProgram(gl) {
    const vertex = compileShader(
      gl,
      gl.VERTEX_SHADER,
      `#version 300 es
    const vec2 positions[3] = vec2[3](vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    void main(){gl_Position=vec4(positions[gl_VertexID],0.0,1.0);}`,
    );
    const fragment = compileShader(
      gl,
      gl.FRAGMENT_SHADER,
      `#version 300 es
    precision highp float;
    precision highp usampler2D;
    uniform usampler2D indexedFrame;
    uniform sampler2D masterPalette;
    out vec4 color;
    void main(){
      ivec2 point=ivec2(int(gl_FragCoord.x),${String(HARDWARE.height - 1)}-int(gl_FragCoord.y));
      uint index=texelFetch(indexedFrame,point,0).r;
      color=texelFetch(masterPalette,ivec2(int(index),0),0);
    }`,
    );
    const program = requireObject(gl.createProgram(), 'shader program');
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS))
      throw new Error(`WebGL2 link failed: ${gl.getProgramInfoLog(program) ?? 'unknown error'}`);
    return program;
  }
  function compileShader(gl, type, source) {
    const shader = requireObject(gl.createShader(type), 'shader');
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS))
      throw new Error(`WebGL2 shader failed: ${gl.getShaderInfoLog(shader) ?? 'unknown error'}`);
    return shader;
  }
  function configureIndexTexture(gl, texture) {
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.R8UI,
      HARDWARE.width,
      HARDWARE.height,
      0,
      gl.RED_INTEGER,
      gl.UNSIGNED_BYTE,
      null,
    );
  }
  function configurePaletteTexture(gl, texture) {
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA8,
      HARDWARE.paletteSize,
      1,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      Uint8Array.from(MASTER_PALETTE_RGBA),
    );
  }
  function setSampler(gl, program, name, unit) {
    const location = gl.getUniformLocation(program, name);
    if (location === null) throw new Error(`WebGL2 sampler '${name}' is missing`);
    gl.uniform1i(location, unit);
  }
  function requireObject(value, description) {
    if (value === null) throw new Error(`WebGL2 could not create ${description}`);
    return value;
  }
  //#endregion
  //#region src/input.ts
  var BUTTONS = ['up', 'down', 'left', 'right', 'a', 'b', 'x', 'y', 'l', 'r', 'start', 'menu'];
  function isButton(value) {
    return typeof value === 'string' && BUTTONS.includes(value);
  }
  var DEFAULT_KEY_BINDINGS = Object.freeze({
    ArrowUp: [0, 'up'],
    ArrowDown: [0, 'down'],
    ArrowLeft: [0, 'left'],
    ArrowRight: [0, 'right'],
    KeyZ: [0, 'a'],
    KeyX: [0, 'b'],
    KeyA: [0, 'x'],
    KeyS: [0, 'y'],
    KeyQ: [0, 'l'],
    KeyW: [0, 'r'],
    Enter: [0, 'start'],
    Escape: [0, 'menu'],
    KeyI: [1, 'up'],
    KeyK: [1, 'down'],
    KeyJ: [1, 'left'],
    KeyL: [1, 'right'],
    KeyF: [1, 'a'],
    KeyG: [1, 'b'],
    KeyR: [1, 'x'],
    KeyT: [1, 'y'],
    KeyV: [1, 'l'],
    KeyB: [1, 'r'],
    Digit1: [1, 'start'],
    Backquote: [1, 'menu'],
    Numpad8: [2, 'up'],
    Numpad5: [2, 'down'],
    Numpad4: [2, 'left'],
    Numpad6: [2, 'right'],
    Numpad1: [2, 'a'],
    Numpad2: [2, 'b'],
    Numpad7: [2, 'x'],
    Numpad9: [2, 'y'],
    NumpadAdd: [2, 'l'],
    NumpadSubtract: [2, 'r'],
    NumpadEnter: [2, 'start'],
    NumpadDecimal: [2, 'menu'],
    KeyY: [3, 'up'],
    KeyH: [3, 'down'],
    KeyU: [3, 'left'],
    KeyO: [3, 'right'],
    KeyC: [3, 'a'],
    KeyD: [3, 'b'],
    KeyE: [3, 'x'],
    KeyM: [3, 'y'],
    KeyN: [3, 'l'],
    KeyP: [3, 'r'],
    Digit2: [3, 'start'],
    Digit3: [3, 'menu'],
  });
  function defaultControllerProfile() {
    return {
      revision: 1,
      name: 'DEFAULT',
      keyboard: structuredClone(DEFAULT_KEY_BINDINGS),
      gamepads: [0, 1, 2, 3],
    };
  }
  function isControllerProfile(value) {
    if (
      !isRecord$2(value) ||
      value.revision !== 1 ||
      typeof value.name !== 'string' ||
      value.name.length === 0 ||
      value.name.length > 24 ||
      !isRecord$2(value.keyboard) ||
      Object.keys(value.keyboard).length > BUTTONS.length * 4 ||
      !Array.isArray(value.gamepads) ||
      value.gamepads.length !== 4 ||
      Object.keys(value.gamepads).length !== 4
    )
      return false;
    const assignments = /* @__PURE__ */ new Set();
    for (const [code, binding] of Object.entries(value.keyboard)) {
      if (
        !/^\w{1,32}$/.test(code) ||
        !Array.isArray(binding) ||
        binding.length !== 2 ||
        !Number.isSafeInteger(binding[0]) ||
        binding[0] < 0 ||
        binding[0] > 3 ||
        !isButton(binding[1])
      )
        return false;
      const target = `${String(binding[0])}/${binding[1]}`;
      if (assignments.has(target)) return false;
      assignments.add(target);
    }
    const connected = value.gamepads.filter((index) => index !== null);
    return (
      connected.every((index) => Number.isSafeInteger(index) && index >= 0 && index <= 255) &&
      new Set(connected).size === connected.length
    );
  }
  var GAMEPAD_BUTTONS = Object.freeze({
    up: 12,
    down: 13,
    left: 14,
    right: 15,
    a: 0,
    b: 1,
    x: 2,
    y: 3,
    l: 4,
    r: 5,
    start: 9,
    menu: 8,
  });
  /** Main-thread keyboard, pointer/touch, and standard-gamepad adapter for four controller ports. */
  var BrowserInput = class {
    surface;
    gamepads;
    profile;
    keys = /* @__PURE__ */ new Set();
    pointer = {
      x: 0,
      y: 0,
      primary: false,
      secondary: false,
      inside: false,
    };
    constructor(
      surface,
      gamepads = () => navigator.getGamepads(),
      profile = defaultControllerProfile(),
    ) {
      if (!isControllerProfile(profile)) throw new TypeError('invalid controller profile');
      this.surface = surface;
      this.gamepads = gamepads;
      this.profile = structuredClone(profile);
      globalThis.addEventListener('keydown', this.handleKeyDown);
      globalThis.addEventListener('keyup', this.handleKeyUp);
      globalThis.addEventListener('blur', this.handleBlur);
      surface.addEventListener('pointermove', this.handlePointerMove);
      surface.addEventListener('pointerdown', this.handlePointerDown);
      surface.addEventListener('pointerup', this.handlePointerUp);
      surface.addEventListener('pointercancel', this.handlePointerCancel);
      surface.addEventListener('pointerenter', this.handlePointerEnter);
      surface.addEventListener('pointerleave', this.handlePointerLeave);
      surface.addEventListener('contextmenu', this.handleContextMenu);
    }
    poll() {
      const buttons = profiledControllerButtons(this.profile, this.keys, this.gamepads());
      return {
        controllers: [
          { buttons: buttons[0] },
          { buttons: buttons[1] },
          { buttons: buttons[2] },
          { buttons: buttons[3] },
        ],
        pointer: { ...this.pointer },
      };
    }
    destroy() {
      globalThis.removeEventListener('keydown', this.handleKeyDown);
      globalThis.removeEventListener('keyup', this.handleKeyUp);
      globalThis.removeEventListener('blur', this.handleBlur);
      this.surface.removeEventListener('pointermove', this.handlePointerMove);
      this.surface.removeEventListener('pointerdown', this.handlePointerDown);
      this.surface.removeEventListener('pointerup', this.handlePointerUp);
      this.surface.removeEventListener('pointercancel', this.handlePointerCancel);
      this.surface.removeEventListener('pointerenter', this.handlePointerEnter);
      this.surface.removeEventListener('pointerleave', this.handlePointerLeave);
      this.surface.removeEventListener('contextmenu', this.handleContextMenu);
    }
    handleKeyDown = (event) => {
      if (this.profile.keyboard[event.code] !== void 0) {
        event.preventDefault();
        this.keys.add(event.code);
      }
    };
    handleKeyUp = (event) => {
      if (this.profile.keyboard[event.code] !== void 0) {
        event.preventDefault();
        this.keys.delete(event.code);
      }
    };
    handleBlur = () => {
      this.keys.clear();
      this.pointer = {
        ...this.pointer,
        primary: false,
        secondary: false,
      };
    };
    handlePointerMove = (event) => {
      this.updatePointerPosition(event);
    };
    handlePointerDown = (event) => {
      this.surface.setPointerCapture(event.pointerId);
      this.updatePointerPosition(event);
      this.pointer = {
        ...this.pointer,
        primary: this.pointer.primary || event.button === 0,
        secondary: this.pointer.secondary || event.button === 2,
      };
    };
    handlePointerUp = (event) => {
      this.updatePointerPosition(event);
      this.pointer = {
        ...this.pointer,
        primary: event.button === 0 ? false : this.pointer.primary,
        secondary: event.button === 2 ? false : this.pointer.secondary,
      };
    };
    handlePointerCancel = () => {
      this.pointer = {
        ...this.pointer,
        primary: false,
        secondary: false,
        inside: false,
      };
    };
    handlePointerEnter = () => {
      this.pointer = {
        ...this.pointer,
        inside: true,
      };
    };
    handlePointerLeave = () => {
      this.pointer = {
        ...this.pointer,
        inside: false,
      };
    };
    handleContextMenu = (event) => {
      event.preventDefault();
    };
    updatePointerPosition(event) {
      const bounds = this.surface.getBoundingClientRect();
      const x = Math.floor(((event.clientX - bounds.left) * HARDWARE.width) / bounds.width);
      const y = Math.floor(((event.clientY - bounds.top) * HARDWARE.height) / bounds.height);
      this.pointer = {
        ...this.pointer,
        x: Math.max(0, Math.min(HARDWARE.width - 1, x)),
        y: Math.max(0, Math.min(HARDWARE.height - 1, y)),
        inside: x >= 0 && x < HARDWARE.width && y >= 0 && y < HARDWARE.height,
      };
    }
  };
  function profiledControllerButtons(profile, keys, gamepads) {
    if (!isControllerProfile(profile)) throw new TypeError('invalid controller profile');
    const buttons = Array.from({ length: 4 }, () => emptyButtons());
    for (const code of keys) {
      const binding = profile.keyboard[code];
      if (binding === void 0) continue;
      const controller = buttons[binding[0]];
      if (controller !== void 0) controller[binding[1]] = true;
    }
    for (const gamepad of gamepads) {
      if (gamepad === null) continue;
      const port = profile.gamepads.indexOf(gamepad.index);
      const controller = port < 0 ? void 0 : buttons[port];
      if (controller !== void 0) applyStandardGamepad(controller, gamepad);
    }
    return buttons;
  }
  function emptyButtons() {
    return Object.fromEntries(BUTTONS.map((button) => [button, false]));
  }
  function applyStandardGamepad(buttons, gamepad) {
    for (const button of BUTTONS)
      buttons[button] ||= gamepad.buttons[GAMEPAD_BUTTONS[button]]?.pressed ?? false;
    const horizontal = gamepad.axes[0] ?? 0;
    const vertical = gamepad.axes[1] ?? 0;
    buttons.left ||= horizontal < -0.5;
    buttons.right ||= horizontal > 0.5;
    buttons.up ||= vertical < -0.5;
    buttons.down ||= vertical > 0.5;
  }
  function isRecord$2(value) {
    return typeof value === 'object' && value !== null;
  }
  //#endregion
  //#region src/save.ts
  function isSaveValues(value) {
    if (!isRecord$1(value) || Array.isArray(value)) return false;
    try {
      for (const [key, entry] of Object.entries(value)) {
        validateKey(key);
        validateInteger(entry);
      }
      return encodedBytes(value) <= HARDWARE.saveCapacityBytes;
    } catch {
      return false;
    }
  }
  function sortedValues(values) {
    return Object.fromEntries(
      Object.entries(values).sort(([left], [right]) => left.localeCompare(right)),
    );
  }
  function encodedBytes(values) {
    return new TextEncoder().encode(JSON.stringify(sortedValues(values))).byteLength;
  }
  function validateKey(key) {
    if (
      !/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(key) ||
      ['__proto__', 'constructor', 'prototype'].includes(key)
    )
      throw new TypeError('save keys must be 1-64 canonical ASCII characters');
  }
  function validateInteger(value) {
    if (typeof value !== 'number' || !Number.isSafeInteger(value))
      throw new TypeError('save values must be safe integers');
  }
  function isRecord$1(value) {
    return typeof value === 'object' && value !== null;
  }
  //#endregion
  //#region src/protocol.ts
  function isWorkerResponse(value) {
    if (!isRecord(value) || !isNonNegativeInteger(value.id) || typeof value.type !== 'string')
      return false;
    switch (value.type) {
      case 'loaded':
      case 'restored':
      case 'memory-edited':
        return hasExactKeys(value, ['id', 'type']);
      case 'debug-step':
        return isDebugStepResponse(value);
      case 'snapshot':
        return hasExactKeys(value, ['id', 'type', 'snapshot']);
      case 'memory':
        return (
          hasExactKeys(value, ['id', 'type', 'address', 'bytes', 'regions']) &&
          value.bytes instanceof Uint8Array &&
          value.bytes.length > 0 &&
          value.bytes.length <= 256 &&
          isMemoryRange(value.address, value.bytes.length) &&
          Array.isArray(value.regions) &&
          value.regions.length <= 4096 &&
          value.regions.every(isMemoryRegionDescriptor)
        );
      case 'frame':
        return (
          hasExactKeys(value, [
            'id',
            'type',
            'frame',
            'workUnits',
            'attribution',
            'drawCommands',
            'audioCommands',
            'saveWrites',
            'output',
            ...(value.saveCommit === void 0 ? [] : ['saveCommit']),
            ...(value.debug === void 0 ? [] : ['debug']),
          ]) &&
          isNonNegativeInteger(value.frame) &&
          isNonNegativeInteger(value.workUnits) &&
          Array.isArray(value.attribution) &&
          value.attribution.every(isAttribution) &&
          Array.isArray(value.drawCommands) &&
          value.drawCommands.every(isConsoleCommand) &&
          Array.isArray(value.audioCommands) &&
          value.audioCommands.every(isConsoleCommand) &&
          Array.isArray(value.saveWrites) &&
          value.saveWrites.every(isSaveWrite) &&
          (value.saveCommit === void 0 ||
            (value.saveCommit instanceof Uint8Array &&
              value.saveCommit.length === HARDWARE.saveCapacityBytes)) &&
          isConsoleOutput(value.output) &&
          (value.debug === void 0 || isDebugFrame(value.debug))
        );
      case 'audit':
        return (
          hasExactKeys(value, ['id', 'type', 'exposedCapabilities', 'mathRandomAvailable']) &&
          Array.isArray(value.exposedCapabilities) &&
          value.exposedCapabilities.every((capability) => typeof capability === 'string') &&
          typeof value.mathRandomAvailable === 'boolean'
        );
      case 'error':
        return (
          hasExactKeys(
            value,
            value.sourceSpan === void 0
              ? ['id', 'type', 'code', 'message']
              : ['id', 'type', 'code', 'message', 'sourceSpan'],
          ) &&
          typeof value.code === 'string' &&
          typeof value.message === 'string' &&
          (value.sourceSpan === void 0 || isSourceSpan(value.sourceSpan))
        );
      default:
        return false;
    }
  }
  function isMemoryRange(address, length) {
    return (
      typeof address === 'number' &&
      Number.isSafeInteger(address) &&
      address >= 0 &&
      typeof length === 'number' &&
      Number.isSafeInteger(length) &&
      length > 0 &&
      length <= 256 &&
      address <= 4194304 - length
    );
  }
  function isMemoryRegionDescriptor(value) {
    return (
      isRecord(value) &&
      hasExactKeys(value, ['name', 'address', 'length', 'writable']) &&
      typeof value.name === 'string' &&
      value.name.length > 0 &&
      value.name.length <= 128 &&
      typeof value.writable === 'boolean' &&
      typeof value.address === 'number' &&
      Number.isSafeInteger(value.address) &&
      value.address >= 0 &&
      typeof value.length === 'number' &&
      Number.isSafeInteger(value.length) &&
      value.length > 0 &&
      value.address <= 4194304 - value.length
    );
  }
  function isConsoleOutput(value) {
    return (
      isRecord(value) &&
      hasExactKeys(value, ['indexedPixels', 'audio', 'audioState']) &&
      value.indexedPixels instanceof Uint8Array &&
      value.indexedPixels.length === HARDWARE.width * HARDWARE.height &&
      value.indexedPixels.every((color) => color < HARDWARE.paletteSize) &&
      isAudioFrame(value.audio) &&
      isSynthSnapshot(value.audioState)
    );
  }
  function isDebugFrame(value) {
    if (
      !isRecord(value) ||
      !hasExactKeys(value, ['trace', 'truncated', 'inspection']) ||
      !Array.isArray(value.trace) ||
      typeof value.truncated !== 'boolean' ||
      !isRecord(value.inspection) ||
      !hasExactKeys(value.inspection, ['state', 'tasks', 'callStack'])
    )
      return false;
    return value.trace.every(isDebugTraceEvent);
  }
  function isDebugStepResponse(value) {
    if (value.booted === true) return hasExactKeys(value, ['id', 'type', 'booted']);
    if (value.frame !== void 0)
      return (
        hasExactKeys(value, ['id', 'type', 'frame']) &&
        isRecord(value.frame) &&
        isWorkerResponse({
          id: value.id,
          type: 'frame',
          ...value.frame,
        })
      );
    return (
      hasExactKeys(value, ['id', 'type', 'event', 'inspection']) &&
      isDebugTraceEvent(value.event) &&
      isRecord(value.inspection) &&
      hasExactKeys(value.inspection, ['state', 'tasks', 'callStack'])
    );
  }
  function isDebugTraceEvent(value) {
    return (
      isRecord(value) &&
      hasExactKeys(value, ['id', 'sourceSpan', 'locals', 'callStack']) &&
      isNonNegativeInteger(value.id) &&
      isSourceSpan(value.sourceSpan) &&
      Array.isArray(value.callStack) &&
      value.callStack.every(
        (frame) =>
          isRecord(frame) &&
          hasExactKeys(frame, ['name', 'sourceSpan']) &&
          typeof frame.name === 'string' &&
          isSourceSpan(frame.sourceSpan),
      )
    );
  }
  function isRecord(value) {
    return typeof value === 'object' && value !== null;
  }
  function isNonNegativeInteger(value) {
    return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
  }
  function isSourceSpan(value) {
    return (
      isRecord(value) &&
      hasExactKeys(
        value,
        value.source === void 0 ? ['start', 'end'] : ['source', 'start', 'end'],
      ) &&
      (value.source === void 0 || isSourceName(value.source)) &&
      isNonNegativeInteger(value.start) &&
      isNonNegativeInteger(value.end) &&
      value.start <= value.end
    );
  }
  function isSourceName(value) {
    if (typeof value !== 'string' || value.length === 0 || value.length > 512) return false;
    for (let index = 0; index < value.length; index += 1) {
      const code = value.charCodeAt(index);
      if (code <= 31 || code === 127) return false;
    }
    return true;
  }
  function isAttribution(value) {
    return (
      isRecord(value) &&
      hasExactKeys(value, ['sourceSpan', 'units']) &&
      isSourceSpan(value.sourceSpan) &&
      isNonNegativeInteger(value.units)
    );
  }
  function isConsoleCommand(value) {
    return (
      isRecord(value) &&
      hasExactKeys(
        value,
        value.rasterLine === void 0
          ? ['name', 'arguments', 'sourceSpan']
          : ['name', 'arguments', 'sourceSpan', 'rasterLine'],
      ) &&
      typeof value.name === 'string' &&
      Array.isArray(value.arguments) &&
      isSourceSpan(value.sourceSpan) &&
      (value.rasterLine === void 0 ||
        (isNonNegativeInteger(value.rasterLine) && value.rasterLine < HARDWARE.height))
    );
  }
  function isSaveWrite(value) {
    return (
      isRecord(value) &&
      hasExactKeys(value, ['key', 'value']) &&
      typeof value.key === 'string' &&
      isSaveValues({ [value.key]: value.value })
    );
  }
  function hasExactKeys(value, expected) {
    const actual = Object.keys(value).sort();
    return actual.length === expected.length && expected.every((key) => actual.includes(key));
  }
  //#endregion
  //#region src/sandbox.ts
  /** Host-side lifecycle for one disposable cartridge worker. */
  var SandboxSession = class {
    worker;
    timeoutMilliseconds;
    pending = /* @__PURE__ */ new Map();
    nextRequestId = 1;
    disposed = false;
    constructor(worker, timeoutMilliseconds = 500) {
      if (!Number.isSafeInteger(timeoutMilliseconds) || timeoutMilliseconds < 10)
        throw new RangeError('sandbox timeout must be an integer of at least 10 milliseconds');
      this.worker = worker;
      this.timeoutMilliseconds = timeoutMilliseconds;
      this.worker.addEventListener('message', this.handleMessage);
      this.worker.addEventListener('error', this.handleWorkerError);
      this.worker.addEventListener('messageerror', this.handleMessageError);
    }
    async load(javascript, configuration) {
      const moduleUrl = URL.createObjectURL(new Blob([javascript], { type: 'text/javascript' }));
      try {
        const response = await this.request((id) => ({
          id,
          type: 'load',
          moduleUrl,
          configuration,
        }));
        this.expectResponse(response, 'loaded');
      } finally {
        URL.revokeObjectURL(moduleUrl);
      }
    }
    async frame(input) {
      const response = await this.request((id) => ({
        id,
        type: 'frame',
        input,
      }));
      this.expectResponse(response, 'frame');
      return response;
    }
    async debugStep(input) {
      const response = await this.request((id) => ({
        id,
        type: 'debug-step',
        input,
      }));
      this.expectResponse(response, 'debug-step');
      return response;
    }
    async snapshot() {
      const response = await this.request((id) => ({
        id,
        type: 'snapshot',
      }));
      this.expectResponse(response, 'snapshot');
      return response.snapshot;
    }
    async audit() {
      const response = await this.request((id) => ({
        id,
        type: 'audit',
      }));
      this.expectResponse(response, 'audit');
      return response;
    }
    async restore(snapshot) {
      const response = await this.request((id) => ({
        id,
        type: 'restore',
        snapshot,
      }));
      this.expectResponse(response, 'restored');
    }
    async inspectMemory(address, length) {
      const response = await this.request((id) => ({
        id,
        type: 'memory',
        address,
        length,
      }));
      this.expectResponse(response, 'memory');
      return response;
    }
    async editMemory(address, bytes) {
      const response = await this.request((id) => ({
        id,
        type: 'memory-edit',
        address,
        bytes,
      }));
      this.expectResponse(response, 'memory-edited');
    }
    dispose() {
      this.shutdown(/* @__PURE__ */ new Error('sandbox worker was disposed'));
    }
    handleMessage = (event) => {
      if (!isWorkerResponse(event.data)) {
        this.disposeWithError(
          /* @__PURE__ */ new Error('sandbox worker returned an invalid protocol message'),
        );
        return;
      }
      const pending = this.pending.get(event.data.id);
      if (pending === void 0) return;
      clearTimeout(pending.timer);
      this.pending.delete(event.data.id);
      if (event.data.type === 'error')
        pending.reject(
          new RuntimeFault(
            event.data.code,
            event.data.message,
            event.data.sourceSpan ?? {
              start: 0,
              end: 0,
            },
          ),
        );
      else pending.resolve(event.data);
    };
    handleWorkerError = (event) => {
      this.disposeWithError(/* @__PURE__ */ new Error(`sandbox worker failed: ${event.message}`));
    };
    handleMessageError = () => {
      this.disposeWithError(
        /* @__PURE__ */ new Error('sandbox worker returned data that could not be cloned'),
      );
    };
    request(create) {
      if (this.disposed)
        return Promise.reject(/* @__PURE__ */ new Error('sandbox worker is disposed'));
      const id = this.nextRequestId;
      this.nextRequestId += 1;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          this.pending.delete(id);
          reject(/* @__PURE__ */ new Error('sandbox worker exceeded its host response deadline'));
          this.dispose();
        }, this.timeoutMilliseconds);
        this.pending.set(id, {
          resolve,
          reject,
          timer,
        });
        this.worker.postMessage(create(id));
      });
    }
    expectResponse(response, expected) {
      if (response.type !== expected)
        throw new Error(`sandbox protocol expected '${expected}', received '${response.type}'`);
    }
    disposeWithError(error) {
      this.shutdown(error);
    }
    shutdown(reason) {
      if (this.disposed) return;
      this.disposed = true;
      this.worker.removeEventListener('message', this.handleMessage);
      this.worker.removeEventListener('error', this.handleWorkerError);
      this.worker.removeEventListener('messageerror', this.handleMessageError);
      this.worker.terminate();
      this.rejectAll(reason);
    }
    rejectAll(error) {
      for (const request of this.pending.values()) {
        clearTimeout(request.timer);
        request.reject(error);
      }
      this.pending.clear();
    }
  };
  //#endregion
  //#region src/standalone-player.ts
  var payload = globalThis.__MOD01_CARTRIDGE__;
  var canvas = requireElement('#screen');
  var status = requireElement('#status');
  var soundButton = requireElement('#sound');
  var pauseButton = requireElement('#pause');
  var resetButton = requireElement('#reset');
  var fullscreenButton = requireElement('#fullscreen');
  var sourceButton = requireElement('#source');
  var inspector = requireElement('#inspector');
  var sourceSelect = requireElement('#source-file');
  var sourceView = requireElement('#source-view');
  var files = Object.fromEntries(
    Object.entries(payload.files).map(([path, value]) => [path, decodeBase64(value)]),
  );
  var sourceEntries = Object.keys(files)
    .filter((path) => path.startsWith('source/'))
    .sort();
  for (const path of sourceEntries) {
    const option = document.createElement('option');
    option.value = path;
    option.textContent = path.slice(7);
    sourceSelect.append(option);
  }
  var showSource = () => {
    const path = sourceSelect.value || sourceEntries[0];
    sourceView.textContent = path === void 0 ? 'NO SOURCE' : new TextDecoder().decode(files[path]);
  };
  sourceSelect.addEventListener('change', showSource);
  sourceButton.addEventListener('click', () => {
    inspector.hidden = !inspector.hidden;
    sourceButton.textContent = inspector.hidden ? 'SOURCE' : 'PLAY';
    showSource();
  });
  requireElement('#close-source').addEventListener('click', () => {
    inspector.hidden = true;
    sourceButton.textContent = 'SOURCE';
  });
  var renderer = new WebGlIndexedRenderer(canvas);
  var sandbox;
  var input;
  var audio;
  var stopped = false;
  var paused = false;
  var generation = 0;
  var isPaused = () => paused;
  document.documentElement.dataset.embed = String(location.hash === '#embed');
  soundButton.addEventListener('click', () => {
    audio = new WebAudioSink();
    audio.resume().then(() => {
      soundButton.textContent = 'SOUND ON';
      soundButton.disabled = true;
    });
  });
  pauseButton.addEventListener('click', () => {
    paused = !paused;
    pauseButton.textContent = paused ? 'RESUME' : 'PAUSE';
    if (paused) {
      status.textContent = 'PAUSED';
      closeAudio();
    }
  });
  resetButton.addEventListener('click', () => {
    restart().catch(showError);
  });
  fullscreenButton.addEventListener('click', () => {
    document.querySelector('.unit')?.requestFullscreen();
  });
  globalThis.addEventListener('pagehide', () => {
    stopped = true;
    generation += 1;
    input?.destroy();
    sandbox?.dispose();
    if (audio !== void 0) audio.close();
  });
  restart().catch(showError);
  async function restart() {
    generation += 1;
    const currentGeneration = generation;
    input?.destroy();
    sandbox?.dispose();
    await closeAudio();
    stopped = false;
    paused = false;
    pauseButton.textContent = 'PAUSE';
    soundButton.textContent = 'SOUND';
    soundButton.disabled = false;
    const nextSandbox = new SandboxSession(
      new WorkerWrapper({
        name: `mod01-standalone-${payload.manifest.id}-${String(currentGeneration)}`,
      }),
      1e3,
    );
    const nextInput = new BrowserInput(canvas);
    sandbox = nextSandbox;
    input = nextInput;
    const javascript = new TextDecoder().decode(requireFile('build/cartridge.js'));
    const rom = decodeBase64(payload.rom);
    await nextSandbox.load(javascript, {
      seed: 1295003697,
      workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
      updateRate: payload.manifest.updateRate,
      assets: {
        declarations: payload.manifest.assets,
        files,
        displayPath: payload.manifest.display,
      },
      save: readSave(),
      rom,
    });
    canvas.focus();
    requestAnimationFrame(() => void frame(currentGeneration, nextSandbox, nextInput));
  }
  async function frame(currentGeneration, currentSandbox, currentInput) {
    if (stopped || currentGeneration !== generation) return;
    if (paused) {
      requestAnimationFrame(() => void frame(currentGeneration, currentSandbox, currentInput));
      return;
    }
    try {
      const result = await currentSandbox.frame(currentInput.poll());
      if (currentGeneration !== generation) return;
      if (isPaused()) {
        requestAnimationFrame(() => void frame(currentGeneration, currentSandbox, currentInput));
        return;
      }
      renderer.render(result.output.indexedPixels);
      audio?.enqueue(result.output.audio);
      if (result.saveCommit !== void 0) writeSave(result.saveCommit);
      status.textContent = `F${String(result.frame).padStart(5, '0')} W${String(result.workUnits).padStart(5, '0')}`;
      requestAnimationFrame(() => void frame(currentGeneration, currentSandbox, currentInput));
    } catch (error) {
      if (currentGeneration !== generation) return;
      stopped = true;
      showError(error);
    }
  }
  async function closeAudio() {
    const current = audio;
    audio = void 0;
    if (current !== void 0) await current.close();
  }
  function readSave() {
    const current = localStorage.getItem(`mod01/v1/${payload.manifest.id}`);
    if (current !== null)
      try {
        const bytes = decodeBase64(current);
        if (bytes.length <= HARDWARE.saveCapacityBytes) return bytes;
      } catch {}
    const alpha = localStorage.getItem(`mod01/${payload.manifest.id}`);
    if (alpha === null) return /* @__PURE__ */ new Uint8Array();
    try {
      const parsed = JSON.parse(alpha);
      if (!isIntegerSave(parsed)) return /* @__PURE__ */ new Uint8Array();
      return new TextEncoder().encode(JSON.stringify(sortRecord(parsed)));
    } catch {
      return /* @__PURE__ */ new Uint8Array();
    }
  }
  function writeSave(bytes) {
    localStorage.setItem(`mod01/v1/${payload.manifest.id}`, encodeBase64(bytes));
  }
  function isIntegerSave(value) {
    return (
      typeof value === 'object' &&
      value !== null &&
      !Array.isArray(value) &&
      Object.keys(value).every(
        (key) =>
          /^[A-Za-z_][A-Za-z0-9_]{0,31}$/.test(key) &&
          typeof value[key] === 'number' &&
          Number.isSafeInteger(value[key]),
      )
    );
  }
  function sortRecord(value) {
    return Object.fromEntries(
      Object.entries(value).sort(([left], [right]) => left.localeCompare(right)),
    );
  }
  function requireFile(path) {
    const bytes = files[path];
    if (bytes === void 0) throw new Error(`standalone cartridge is missing '${path}'`);
    return bytes;
  }
  function showError(error) {
    status.textContent = error instanceof Error ? error.message : 'MOD-01 standalone failed';
    status.classList.add('error');
  }
  function decodeBase64(value) {
    const decoded = atob(value);
    return Uint8Array.from(decoded, (character) => character.charCodeAt(0));
  }
  function encodeBase64(value) {
    let binary = '';
    for (const byte of value) binary += String.fromCharCode(byte);
    return btoa(binary);
  }
  function requireElement(selector) {
    const element = document.querySelector(selector);
    if (element === null) throw new Error(`standalone player is missing '${selector}'`);
    return element;
  }
  //#endregion
})();
